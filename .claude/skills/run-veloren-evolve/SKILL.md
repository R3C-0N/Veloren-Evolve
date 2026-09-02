---
name: run-veloren-evolve
description: Compiler, lancer, piloter et prendre des captures du client Veloren (voxygen) du fork Evolve sur Windows. À utiliser pour run/start/build/launch/screenshot/tester le jeu, entrer en solo, créer un monde ou un personnage, ou vérifier qu'une modification se voit à l'écran.
---

# Lancer et piloter Veloren-Evolve

Client natif Rust + wgpu, rendu **Vulkan**, fenêtre winit. Il n'y a pas de
surface web ni de REPL : le pilotage passe par un script PowerShell qui force
le focus, injecte souris et clavier au niveau du système, et capture l'aire
client de la fenêtre.

**Le pilote est `.claude/skills/run-veloren-evolve/driver.ps1`.** Tous les
chemins ci-dessous sont relatifs à `Veloren-Evolve/`. Environnement vérifié :
Windows 11, écran 1920×1080 à **125 %**.

## Prérequis

Vérifier que le nightly épinglé est **complet** — `rustc.exe` doit être là.
Une installation interrompue laisse `cargo.exe` mais pas `rustc.exe`, et
`cargo build` échoue sur `Missing manifest in toolchain` :

```powershell
ls "$env:USERPROFILE\.rustup\toolchains\nightly-2026-06-13-x86_64-pc-windows-msvc\bin\rustc.exe"
```

S'il manque, réinstaller :

```powershell
rustup toolchain uninstall nightly-2026-06-13-x86_64-pc-windows-msvc
rustup toolchain install   nightly-2026-06-13-x86_64-pc-windows-msvc --profile default
```

`shaderc` est compilé depuis ses sources (`shaderc-from-source` est dans les
features **par défaut** de voxygen) : il faut cmake, Python et **ninja**.

```powershell
python -m pip install ninja
ninja --version
```

## Build

```powershell
cargo build --profile no_overflow --bin veloren-voxygen
```

Profil `no_overflow` volontairement : il désactive les contrôles de
débordement pour la génération de monde tout en gardant un temps de
compilation raisonnable. `--release` ajoute du LTO sur tout le workspace pour
un gain nul ici.

Compter **~10 min de téléchargement + ~15 min de compilation** au premier
build (≈ 900 crates). Ensuite c'est incrémental.

**Ne pas lancer ce build via un outil qui coupe à 10 minutes** — il sera tué en
plein téléchargement. Le détacher :

```powershell
Start-Process -FilePath "$env:USERPROFILE\.cargo\bin\cargo.exe" `
  -ArgumentList "build","--profile","no_overflow","--bin","veloren-voxygen" `
  -WorkingDirectory (Get-Location) `
  -RedirectStandardOutput "$env:TEMP\veloren-run\build.out" `
  -RedirectStandardError  "$env:TEMP\veloren-run\build.err" -NoNewWindow -PassThru
```

Puis surveiller `build.err` : `Finished` = succès, `^error` = échec.

## Lancer et piloter (chemin agent)

```powershell
$d = ".\.claude\skills\run-veloren-evolve\driver.ps1"

pwsh -File $d -Action launch          # démarre, attend la fin des shaders, place la fenêtre
pwsh -File $d -Action rapide          # démarre **et entre en jeu**, sans toucher aux menus
pwsh -File $d -Action rapide -Monde "Cube 42"   # le même, sur un monde nommé
pwsh -File $d -Action state           # PID, géométrie, RAM, chemin du journal
pwsh -File $d -Action shot -Out shot.png
pwsh -File $d -Action stop            # WM_CLOSE, puis force après 10 s
```

`launch` compte 40 à 60 s : voxygen compile ~45 pipelines GLSL au démarrage.
Il attend la ligne `egui_wgpu` du journal, qui suit la dernière compilation.

**`rapide` est le chemin normal pour un essai.** Il passe `--partie-rapide` au
client, qui reprend le **dernier** monde solo et le **premier** personnage, et
en crée au besoin — six écrans de menu en moins, et surtout aucune coordonnée
de clic à relire. Il attend la ligne `partie rapide : on entre avec`, pas un
délai : une génération de monde neuf prend une à deux minutes, et un délai fixe
mentirait dans les deux sens. Compter jusqu'à 5 min sur un monde à générer,
~1 min sur un monde déjà là.

`-Monde <nom>` choisit le monde par son nom. **Un nom inconnu arrête au menu**
plutôt que d'en ouvrir un autre : entrer dans le mauvais monde est pire que ne
pas entrer, parce qu'on mesure alors le mauvais terrain sans le savoir.

Le personnage créé d'office s'appelle `Cobaye`, corps humanoïde tiré au sort,
épée de départ. Le monde créé d'office est un monde carré ordinaire : **pour un
monde cubique, il faut encore passer par `Create Custom` une fois**, puis
`rapide` le retrouve.

**Les coordonnées de `click` sont exactement celles lues sur l'image de
`shot`** — la capture est limitée à l'aire client, sans barre de titre. La
boucle de travail est : `shot`, regarder l'image, `click` sur ce qu'on y voit.

```powershell
pwsh -File $d -Action click -X 740 -Y 322
pwsh -File $d -Action drag  -X 1288 -Y 400 -X2 560 -Y2 857  # d'une case à l'autre
pwsh -File $d -Action key   -Value esc      # nommées : enter esc space tab f1 f4 f11 up down left right back del
pwsh -File $d -Action key   -Value 1        # tout le reste : un caractère, pris dans la disposition
pwsh -File $d -Action text  -Value "Evolve"
pwsh -File $d -Action look  -Dx 500 -Dy 60  # caméra, mouvement relatif
pwsh -File $d -Action zoom  -Ticks -6       # négatif = reculer la caméra
pwsh -File $d -Action walk  -Value w -Seconds 4
pwsh -File $d -Action fit                   # replace la fenêtre si elle a bougé
```

**`key` passe par la disposition du clavier, et c'est indispensable.** Veloren
lie ses entrées à des caractères *logiques* — `Key::Character("2")` pour le
deuxième emplacement de la barre d'objets. Sur un clavier AZERTY la rangée de
chiffres demande Maj : envoyer le code virtuel `0x32` y produit « é », que le
jeu ignore **en silence**. La touche semble alors perdue, et on cherche le
défaut dans le code du jeu. Seules les touches nommées gardent un code fixe.

**`drag` est le seul geste qui garnit la barre d'objets** : depuis la refonte
du sac, un clic simple sur un objet n'ouvre qu'un menu *Use / Drop / Cancel*.

Sorties et journaux : `%TEMP%\veloren-run\` (`game.out`, `game.err`, captures).
Le jeu y écrit aussi ses propres captures F4, via `VOXYGEN_SCREENSHOT`.

### Aller jusqu'en jeu

Enchaînement vérifié, coordonnées valables à la taille par défaut
(client 1482×883). **Re-lire les coordonnées sur un `shot` à chaque étape** :
elles bougent avec la taille de fenêtre et avec l'état du monde.

| Étape | Action |
|---|---|
| Menu principal | `click -X 731 -Y 470` (Singleplayer) |
| Liste des mondes | `click -X 187 -Y 785` (New) |
| Monde créé | `click -X 183 -Y 127` (le sélectionner) |
| Panneau du monde | `click -X 562 -Y 189` (Play) → génération, **1 à 2 min** |
| Sélection de personnage | `click -X 170 -Y 160` (Create new character, liste vide) |
| Création | `click -X 741 -Y 844`, `text -Value "<nom>"`, `click -X 1415 -Y 861` (Create) |
| Retour à la sélection | `click -X 923 -Y 851` (Enter World) → ~30 s de chargement |

Deux écrans changent selon ce qui existe déjà : le bouton du milieu du panneau
de monde devient `Regenerate` au lieu de `Create Custom` pour un monde déjà
généré, et `Create new character` descend à `-Y 235` dès qu'un personnage
figure dans la liste. Raison de plus pour relire un `shot` à chaque étape.

Au premier `Play`, **le pare-feu Windows ouvre une boîte de dialogue** qui
n'appartient pas au jeu. Le solo passe par la boucle locale et n'a pas besoin
de l'accès réseau : refuser suffit. C'est une fenêtre séparée, donc `click` du
pilote ne l'atteint pas — cliquer en coordonnées écran absolues.

### Éprouver la construction

Casser et poser sont derrière le mode construction, et rien d'autre — ni zone à
déclarer ni permission à accorder, depuis D32. Une seule commande en chat
(`enter`, puis `text`, puis `enter`) :

```
/build
```

Ensuite, viser au réticule : `right` pose, `middle` est la pipette. La matière
posée est celle de l'emplacement **sélectionné** dans la barre d'objets —
`key -Value 1` le choisit, et `drag` l'y a mis.

**Casser se maintient, depuis D34.** Un `press -Button left` nu dure 110 ms et
n'entame plus rien : il faut `-Seconds`, et assez longtemps pour la dureté du
bloc et le grade de l'outil.

```powershell
pwsh -File $d -Action press -Button left -Seconds 3   # creuse 3 s
```

Ordres de grandeur, à mains nues : herbe et feuillage 0,4 s, terre et sable
0,6 s, bois et glace 2 s, roche 5 s, pierre dure 12 s, obsidienne 30 s. Un bon
outil divise par 2,5 (bois), 4 (pierre) ou 5,5 (métal) ; le mauvais outil ne
divise rien.

**Une jauge entoure le réticule** pendant le creusement — c'est l'anneau de
charge, réemployé. Un `shot` en plein creusement la montre, et c'est le seul
moyen de voir que le geste porte avant que le bloc ne parte.

**Un bloc peut partir sans rien lâcher** : c'est le verrou de grade, pas une
panne. Creuser de la roche à mains nues fait exactement cela. Lire le sac, pas
le décor.

Le réticule ne vise plus qu'à **huit blocs** : au-delà, il n'accroche aucun
bloc, et c'est voulu — il s'arrête exactement là où le serveur cesse
d'accepter. Un clic qui ne fait rien de loin n'est donc pas une panne.

**Ce qui prouve la pose, c'est le compte de la pile, pas le décor** : un bloc
de terre posé sur de la terre ne se voit pas. Lire le compte sur l'icône de la
barre, ou le `n/36` du sac, avant et après. Recadrer la barre d'objets
(`x 510..980`, `y 838..883` en 1482×883) et l'agrandir au plus proche voisin
rend le chiffre lisible.

## Chemin humain

`cargo run --profile no_overflow --bin veloren-voxygen`, la fenêtre s'ouvre,
on joue. `Échap` ouvre le menu, `F4` prend une capture, `F11` bascule le plein
écran. Inutile pour un agent : rien n'est observable sans le pilote.

## Pièges

- **Le plein écran sans bordure rend la capture aveugle.** Windows passe ces
  fenêtres en *independent flip*, qui court-circuite le compositeur ; GDI ne
  lit plus qu'un tampon vide, uniformément blanc. `launch` force
  `fullscreen: enabled: false` dans `userdata/voxygen/settings.ron`. **Ne pas
  appuyer sur F11** pendant une session pilotée.
- **Copier le seul rectangle client ressort parfois tout blanc.** La même
  copie prise en plein écran, puis recadrée, rend l'image juste — c'est ce que
  fait `shot`. Une capture blanche n'est pas une erreur : c'est une image
  plausible, donc un alibi. On conclut que le jeu ne dessine plus, ou que les
  clics sont perdus, alors qu'il joue normalement.
- **Même en fenêtré, le swapchain Vulkan fige la capture.** Deux `shot` à une
  minute d'écart sortaient identiques *au bit près* alors que le jeu
  consommait du CPU et que son état avait changé. Réappliquer `SetWindowPos`
  force DWM à recomposer : `shot` le fait avant chaque capture. Sans ce
  réveil, **la capture ment** — et elle ment en montrant un écran plausible,
  ce qui fait conclure à tort que les entrées sont perdues.
- **`SetProcessDPIAware` est obligatoire.** L'écran est à 125 % : sans cet
  appel, PowerShell voit un bureau virtualisé de 1536×864 et les clics tombent
  25 % trop loin. Le pilote l'appelle en tête.
- **La fenêtre ne doit jamais déborder sous la barre des tâches.** À la taille
  d'origine elle descendait à y=1134 sur un écran de 1080 : les boutons du bas
  (`New`, `Play`, `Enter World`) recevaient des clics qui allaient à la barre
  des tâches. `launch` et `fit` la calent à 1500×930 en haut à gauche.
- **`SetForegroundWindow` seul ne suffit pas** à faire arriver les entrées,
  même quand `GetForegroundWindow` désigne déjà la fenêtre. Il faut
  `AttachThreadInput` sur le fil du premier plan *et* celui du jeu ; le pilote
  encadre chaque action avec.
- **Un saut sec du curseur ne produit pas toujours de `WM_MOUSEMOVE`**, et
  l'interface ne sait alors pas ce qui est survolé. `click` approche la cible
  en quatre pas.
- **La molette : `-120` ne rentre pas dans un `uint32`.** Passer `4294967176`.
- **En jeu le curseur est capturé** : la caméra se pilote en mouvement
  *relatif* (`look`), pas en position absolue. Pour cliquer sur un élément
  d'interface en jeu, ouvrir d'abord le menu (`key -Value esc`).

## Dépannage

| Symptôme | Cause et correctif |
|---|---|
| `error: Missing manifest in toolchain` | Nightly à moitié installé, `rustc.exe` absent. Réinstaller (voir Prérequis). |
| `couldn't find required command: "ninja"` | `python -m pip install ninja` |
| Capture uniformément blanche | Plein écran sans bordure. `-Action launch` le désactive ; sinon mettre `enabled: false` dans le bloc `fullscreen` de `userdata/voxygen/settings.ron`. |
| Deux captures identiques, le jeu a pourtant bougé | Capture figée : utiliser `-Action shot` (il réveille le compositeur), jamais un `CopyFromScreen` nu. |
| Les clics ne font rien | Trois causes à écarter dans cet ordre : DPI (le pilote s'en charge), fenêtre débordant sous la barre des tâches (`-Action fit`), focus (`AttachThreadInput`, dans le pilote). |
| `cargo build` renvoie 0 mais rien n'est bâti | Un `| tee` masque le code de sortie. Poser `set -o pipefail`, ou lire `Finished` dans le journal. |
| Le process tourne mais aucune fenêtre après 3 min | Lire `%TEMP%\veloren-run\game.err`. |

## Note dépôt

`Veloren-Evolve/` est un submodule. Committer ce skill demande deux commits :
un ici, puis le déplacement du pointeur dans le dépôt parapluie. `userdata/`
est dans le `.gitignore` du fork — les réglages modifiés par `launch` ne
salissent pas l'arbre.
