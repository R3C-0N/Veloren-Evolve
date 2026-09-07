# Temps de compilation

Un build complet de voxygen part de 690 crates et se termine par les gros
morceaux de la maison — `common`, `world`, `server`, `voxygen` — puis par une
édition de liens sur un exécutable de plusieurs centaines de mégaoctets.

Tout ce qui suit vise le **build de développement**. Les builds de publication
(`--release`, LTO sur tout le graphe) sont lents par construction et c'est très
bien ainsi.

## Les mesures

Quatre builds complets, `cargo clean` avant chacun, même machine (4 cœurs),
profil `no_overflow`, relevés au `cargo build --timings` :

| build | mur | CPU cumulé | unités |
|---|---|---|---|
| features par défaut, shaderc depuis les sources | **18 min 24 s** | 61,8 min | 1026 |
| features réduites, shaderc depuis les sources | 14 min 49 s | 48,1 min | 831 |
| features réduites, shaderc précompilé (`cargo fast-voxygen`) | **10 min 06 s** | 34,7 min | 829 |
| features réduites + `-Z threads=8` | 14 min 41 s | 47,9 min | 827 |

**18 min 24 s → 10 min 06 s, soit 45 %.**

Les crates les plus chers du build par défaut :

```
  357 s  shaderc-sys        (script de build : glslang + SPIRV-Tools, en C++)
  197 s  veloren-server
  145 s  cranelift-codegen  ← greffons
  133 s  veloren-world
  130 s  veloren-common
  122 s  veloren-voxygen
   92 s  syn
   74 s  wasmtime           ← greffons
```

Deux enseignements. D'abord, **le crate le plus cher du build n'est pas du
Rust** : c'est la compilation C++ de shaderc, et elle est sur le chemin
critique — elle tourne de 626 s à 983 s, et `veloren-voxygen` démarre à 983 s,
exactement quand elle finit. Ensuite, la pile des greffons pèse 9,9 min de CPU,
16 % du total, pour un dépôt qui ne livre aucun greffon.

Le premier point coûte d'ailleurs plus que ses 349 s : la supprimer fait tomber
le CPU cumulé de 48,1 à 34,7 minutes, soit 13,4 minutes, parce que le `ninja`
lancé par le script de build disputait les cœurs à tout ce qui compilait en même
temps. `veloren-server` tombe de 186 s à 99 s sans avoir changé d'une ligne.

## 1. Ne pas compiler shaderc : le SDK Vulkan

**4 min 43 s sur 14 min 49 s, soit 32 %.** Le plus gros levier de la liste, et
il ne se joue pas dans du code Rust.

La feature `shaderc-from-source` compile **glslang et SPIRV-Tools depuis leurs
sources C++**, avec cmake, Python et ninja en prérequis. Le SDK Vulkan livre
exactement la même bibliothèque, déjà construite (`shaderc_combined.lib`).

`shaderc-from-source` est donc absent des alias `fast-voxygen` et `fast-run`.
Sans elle, `shaderc-sys` cherche d'abord une bibliothèque déjà construite : il
lit `$VULKAN_SDK` tout seul, et n'exige que la version 1.2.182 ou plus récente.
S'il n'en trouve aucune, **il retombe de lui-même sur la compilation depuis les
sources** — sans SDK, le comportement est donc celui d'avant, et il n'y a rien
à désinstaller pour revenir en arrière. On peut aussi pointer un autre
répertoire avec `SHADERC_LIB_DIR`.

Installer le SDK Vulkan, c'est donc aussi trois prérequis de moins (cmake,
Python, ninja) et près de cinq minutes sur chaque build complet.

## 2. Compiler moins : `cargo fast-voxygen`

Le second levier mesuré : **20 % du build complet**, cumulable avec le premier.

Les features par défaut de voxygen tirent 690 crates. Elles incluent
`plugins`, qui embarque **wasmtime et cranelift** — un compilateur JIT complet,
95 crates à lui seul — alors que `plugin/` ne contient que des exemples et les
définitions `wit`. Elles incluent aussi `discord` et `native-dialog`, ce
dernier ne servant qu'à afficher une boîte de dialogue quand le jeu panique.

    cargo fast-voxygen          # build
    cargo fast-run              # build + lancement
    cargo fast-run --partie-rapide

L'alias est défini dans `.cargo/config.toml` : profil `no_overflow`, et
`--no-default-features --features singleplayer,simd,hot-reloading,egui-ui`.
Soit **586 crates au lieu de 690**, pour un client qui se joue exactement
pareil en solo. Le gain ne se limite d'ailleurs pas aux crates supprimés :
`veloren-voxygen` lui-même tombe de 122 s à 95 s, parce qu'il a moins de code
sous `cfg` à compiler.

`egui-ui` est gardé exprès : il ne coûte que treize crates, et le pilote de la
compétence `run-veloren-evolve` attend la ligne de journal `egui_wgpu` pour
savoir que le démarrage est fini. Le retirer casserait `driver.ps1 -Action
launch`.

**Ne pas alterner** entre `cargo build` (features par défaut) et
`cargo fast-voxygen` : chaque changement de jeu de features force la
recompilation des crates concernés. Choisir un des deux et s'y tenir.

## 3. Relier plus vite : `rust-lld` sur Windows

`link.exe`, l'éditeur de liens de MSVC, est le poste dominant du build
**incrémental** : après une ligne changée dans voxygen, `cargo build` ne fait
presque plus que relier. `rust-lld` est livré avec le toolchain Rust — il est
donc déjà installé — et fait le même travail plusieurs fois plus vite.

C'est fait, dans `.cargo/config.toml` :

```toml
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```

Sur Linux, le dépôt utilisait déjà `mold` de la même façon. C'est le seul
levier de cette page qui n'a pas été mesuré ici : la machine de mesure est
sous Linux, et y utilisait déjà un éditeur de liens rapide.

## 4. Travailler sur `world` sans compiler voxygen

C'est le plus gros levier pour qui touche à la génération de monde, et il ne
demande aucune configuration : **`veloren-world` ne dépend que de 206 crates**,
contre 586 pour le client. Ni wgpu, ni shaderc, ni conrod, ni iced, ni
l'édition de liens du gros exécutable, ni les 197 s de `veloren-server`.

    cargo run --profile no_overflow -p veloren-world --example empreinte
    cargo run --profile no_overflow -p veloren-world --example reglages
    cargo run --profile no_overflow -p veloren-world --example view

`empreinte` est déjà l'oracle de non-régression du terrain ; `view` affiche la
carte. Une boucle de travail qui reste dans `world` et ne lance le client que
pour vérifier le rendu final coûte une fraction du temps.

Même logique pour `cargo check`, qui ne fait pas de codegen : le tour de piste
des erreurs de compilation se fait sur `cargo check`, pas sur `cargo build`.

## 5. Recharger à chaud plutôt que recompiler

Deux features existent déjà pour éviter un cycle complet :

- `hot-anim` (`anim/use-dyn-lib`) recompile les animations en bibliothèque
  dynamique, sans relier voxygen ;
- `hot-reloading` recharge les assets sans redémarrer — elle est dans le jeu de
  features de `fast-voxygen`.

## Et pour aller plus loin ?

Une fois le build complet à dix minutes, c'est la boucle **incrémentale** qui
gouverne les journées — et elle est déjà bonne. Mesures faites en modifiant
vraiment le code, pas en faisant un `touch` (un `touch` ne change que la date :
le cache incrémental de rustc reconnaît le contenu et ne refait presque rien,
ce qui donne des chiffres flatteurs et faux) :

| modification | crates | rebuild |
|---|---|---|
| corps d'une fonction dans `voxygen` | 1 | **9 s** |
| API publique de `voxygen` | 1 | 57 s |
| n'importe quoi dans `world` | 5 | **16 à 22 s** |
| n'importe quoi dans `common` | 13 | **48 à 51 s** |
| `cargo check -p veloren-world`, en régime établi | — | **1 s** |

Trois choses à en tirer.

**Le premier `cargo check` coûte 91 s, les suivants 1 s.** Il ne partage pas ses
artefacts avec `cargo build` : il se construit son propre cache, une fois. La
boucle « corriger les erreurs du compilateur » se fait donc à la seconde.

**`veloren-common` est le goulot restant.** 154 s à froid, 48 s de rebuild, et
treize crates derrière lui. C'est le seul endroit où un découpage en crates plus
petits changerait vraiment quelque chose — mais c'est un chantier, pas un
réglage.

**Toucher à l'API publique coûte cinq fois le prix d'un corps de fonction.**
Dans `voxygen`, 57 s contre 9 s. Quand on itère, garder les changements à
l'intérieur des corps de fonction le temps de converger, et ne remonter dans les
signatures qu'une fois qu'on sait ce qu'on veut, est gratuit et rapporte.

### Le levier qui reste, et ce qu'on ne sait pas de lui

`[profile.dev.package."*"]` compile les ~600 dépendances en `opt-level = 3`, et
un `TODO` du dépôt dit depuis longtemps que 2 devrait suffire. Mesuré, build
complet et génération de monde (`world_generate_time`, quatre passes) :

| `opt-level` des deps | build complet | génération de monde |
|---|---|---|
| 3 (actuel) | 10 min 06 s | 9,9 / 10,0 / 10,4 / 10,2 s |
| 2 | 9 min 51 s | 10,0 / 9,9 / 10,4 / 10,2 s |
| 1 | **9 min 10 s** | 9,7 / 10,4 / 9,8 / 10,8 s |

Le `TODO` a raison — le niveau 3 ne sert à rien ici — mais il ne rapporte que
15 secondes. C'est **1** qui paie : 56 secondes, 9 % du build, sans régression
mesurable sur la worldgen.

Ce n'est pas un hasard, et c'est ce qui rend le résultat lisible : le code chaud
de la génération de monde ne vit pas dans les dépendances. Il vit dans
`veloren-world`, déjà en `opt-level = 3` par le profil, et dans `veloren-common`.
Et les dépendances qui compteraient — `vek`, `hashbrown`, `num-traits` — sont
génériques : leur code est monomorphisé **dans le crate appelant** et compilé au
niveau de l'appelant, pas au leur. Baisser le leur ne touche presque rien de
chaud.

**Ce n'est pas adopté par défaut, et voici le trou :** ce banc mesure la
génération de monde, pas le client. Le temps par image passe par `wgpu`,
`image`, `conrod`, `specs` — que ce banc n'exerce pas. Avant d'adopter
`opt-level = 1`, il faut une mesure de temps par image ; sans elle, on
échangerait 56 secondes de build contre un risque non mesuré.

### Deux pistes qui demandent du code, pas un réglage

**`libsqlite3-sys`, 49,5 s** de compilation C à chaque build complet. Couper la
feature `persistent_world` ne suffit pas : `rusqlite` est une dépendance *dure*
de `veloren-server` (`server/Cargo.toml:78`), pas conditionnée. Il faudrait la
rendre optionnelle et mettre le module de persistance derrière un `cfg`.

**113 crates sont présents en plusieurs versions** dans le lockfile, dont
`windows-sys` en **six** (0.45, 0.48, 0.52, 0.59, 0.60, 0.61) et
`windows-targets` en quatre. Aucune ne se compile sous Linux — les mesures de ce
fichier ne les voient donc pas, mais un build Windows les paie toutes. Elles
sont sémantiquement incompatibles, cargo ne peut pas les unifier : cela se règle
en faisant monter les dépendances qui retiennent les vieilles versions.

### Le ThinLTO que personne n'a demandé

Le profileur de rustc (`-Z time-passes`) sur `veloren-common`, 98,8 s au total :

```
 32,9 s  finish_ongoing_codegen
 20,6 s  LLVM_thinlto          <-- alors que le profil dit `lto = false`
 20,5 s  LLVM_passes
 18,1 s  lint_checking
 15,0 s  MIR_borrow_checking
 10,0 s  type_check_crate
```

Ce n'est pas une contradiction, c'est un piège de cargo : **`lto = false`
signifie « défaut », et le défaut fait un ThinLTO *local*** entre les unités de
codegen d'un même crate. Seul `lto = "off"` l'éteint vraiment. Passer le profil
`dev` à `lto = "off"` fait tomber le build complet de **10 min 06 s à 7 min
59 s, soit −21 %** — le deuxième plus gros levier de tout ce fichier.

**Et il est à rejeter.** La génération de monde passe de ~10 s à **15,7 à
17,2 s, soit +60 %** :

| | build complet | génération de monde |
|---|---|---|
| `lto = false` (actuel) | 10 min 06 s | 9,9 / 10,0 / 10,4 / 10,2 s |
| `lto = "off"` | 7 min 59 s | 15,7 / 16,2 / 17,2 / 15,7 s |

Deux minutes de build contre soixante pour cent du temps de génération : le
marché est mauvais, et il n'y a pas à hésiter.

Ce résultat explique rétrospectivement celui de l'`opt-level` plus haut. Ce qui
rend ce code rapide, ce n'est pas le niveau d'optimisation appliqué à chaque
unité de codegen — c'est **l'inlining entre unités**, que le ThinLTO local rend
possible. D'où le fait que passer les dépendances de 3 à 1 ne coûte rien, et que
couper le ThinLTO coûte énormément. Les deux mesures disent la même chose.

### Trois pistes qui avaient l'air bonnes et ne rapportent rien

Elles sont ici pour éviter qu'on les reprenne.

**Le groupe de lints `rust_2024_compatibility`.** C'est bien de la configuration
morte — un groupe de lints de *migration* vers l'édition 2024, sur un workspace
déjà en `edition = "2024"` — et `lint_checking` pèse 18,1 s sur `veloren-common`.
Mais en le désactivant, `module_lints` passe de **18,125 s à 18,228 s** : aucun
gain. Les 18 secondes sont la machinerie de lints ordinaire (`unused`,
`dead_code`…), qu'on ne coupera pas.

**Les dépendances déclarées et jamais utilisées.** Il y en a cinq —
`ordered-float` dans `common`, `strum` dans `server`, `sha2` dans `voxygen`,
`num-traits` dans `rtsim`, `futures` dans `common/state` — vérifiées à zéro
occurrence. Mais les quatre premières servent à *d'autres* crates du workspace :
le crate est compilé une fois pour le graphe, retirer la déclaration ne l'enlève
pas du build. La cinquième semblait valoir 8 s à elle seule, puisqu'elle est la
seule à tirer `futures-macro` (6,9 s) — jusqu'à ce que `cargo tree -i` montre
que `iced_futures` la tire aussi. Gain réel : zéro. C'est de l'hygiène de
manifeste, pas de la vitesse.

**`regex` en dépendance de build de `common`.** La pile `regex` est compilée
deux fois (55 s cumulées), et `common/build.rs` la tirait pour un seul test de
forme de tag. Le retrait est fait — le crate n'a plus aucune dépendance de build
— mais il **ne gagne pas une seconde** : le doublon vient de `refinery`, dont la
macro procédurale met `regex` dans le graphe hôte pendant que voxygen l'a dans
le graphe cible. Les deux unités ont exactement les mêmes features avant et
après. Diagnostic juste, cause fausse.

## Ce qui a été essayé et écarté

**`-Z threads=8`, le front-end parallèle de rustc.** L'idée : rustc analyse un
même crate sur plusieurs threads au lieu d'un seul, ce qui devrait rattraper la
queue du build, quand il ne reste que `veloren-voxygen` et que les autres cœurs
tournent à vide. Mesuré : **14 min 41 s avec, 14 min 49 s sans**. Huit secondes
sur quinze minutes, c'est du bruit. Le crate de queue lui-même ne bouge pas
(95,4 s contre 96,2 s). Un drapeau `-Z` instable, qui invalide tout le cache le
jour où on l'ajoute, ne mérite pas d'être dans le dépôt pour ça.

**Baisser l'`opt-level` des dépendances.** Mesuré, et détaillé plus haut : 56
secondes à gagner, sans régression sur la génération de monde, mais sans mesure
du côté du client. En attente d'un banc de temps par image.

**sccache.** Utile pour repartir de zéro souvent (changement de branche,
`cargo clean`), inutile en incrémental — sccache ne met pas en cache les
compilations incrémentales.

**Le backend cranelift de rustc.** Le vrai gain sur les builds de debug, mais
`rustc_codegen_cranelift` n'est pas fiable sur `x86_64-pc-windows-msvc`.

**`--release` pour tester.** Le LTO sur tout le graphe est ce qui coûte le plus
cher, et n'apporte rien de visible ici : `no_overflow` est le bon profil pour
essayer une modification.
