# Temps de compilation

Un build complet de voxygen part de 690 crates et se termine par les gros
morceaux de la maison — `common`, `world`, `server`, `voxygen` — puis par une
édition de liens sur un exécutable de plusieurs centaines de mégaoctets.

Tout ce qui suit vise le **build de développement**. Les builds de publication
(`--release`, LTO sur tout le graphe) sont lents par construction et c'est très
bien ainsi.

## Les mesures

Trois builds complets, `cargo clean` avant chacun, même machine (4 cœurs),
profil `no_overflow`, relevés au `cargo build --timings` :

| build | mur | CPU cumulé | unités |
|---|---|---|---|
| features par défaut | **18 min 24 s** | 61,8 min | 1026 |
| `cargo fast-voxygen` | **14 min 49 s** | 48,1 min | 831 |
| `cargo fast-voxygen` + `-Z threads=8` | 14 min 41 s | 47,9 min | 827 |

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

## 1. Compiler moins : `cargo fast-voxygen`

C'est le levier mesuré ci-dessus : **20 % du build complet**.

Les features par défaut de voxygen tirent 690 crates. Elles incluent
`plugins`, qui embarque **wasmtime et cranelift** — un compilateur JIT complet,
95 crates à lui seul — alors que `plugin/` ne contient que des exemples et les
définitions `wit`. Elles incluent aussi `discord` et `native-dialog`, ce
dernier ne servant qu'à afficher une boîte de dialogue quand le jeu panique.

    cargo fast-voxygen          # build
    cargo fast-run              # build + lancement
    cargo fast-run --partie-rapide

L'alias est défini dans `.cargo/config.toml` : profil `no_overflow`, et
`--no-default-features --features singleplayer,simd,hot-reloading,shaderc-from-source,egui-ui`.
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

## 2. Ne pas compiler shaderc : le SDK Vulkan

La feature `shaderc-from-source` compile **glslang et SPIRV-Tools depuis leurs
sources C++**, avec cmake, Python et ninja en prérequis. 357 s dans le relevé
ci-dessus, l'unité la plus chère du build, et sur le chemin critique.

Le SDK Vulkan livre exactement la même bibliothèque, déjà construite
(`shaderc_combined.lib`). S'il est installé, `shaderc-sys` le trouve tout seul
par la variable `VULKAN_SDK` : il suffit de **retirer `shaderc-from-source`**
des deux alias `fast-voxygen` et `fast-run`. On peut aussi pointer un autre
répertoire avec `SHADERC_LIB_DIR`.

Ce n'est payant qu'au premier build — ensuite l'artefact est en cache dans
`target/` — mais c'est aussi trois prérequis de moins à installer.

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

## Ce qui a été essayé et écarté

**`-Z threads=8`, le front-end parallèle de rustc.** L'idée : rustc analyse un
même crate sur plusieurs threads au lieu d'un seul, ce qui devrait rattraper la
queue du build, quand il ne reste que `veloren-voxygen` et que les autres cœurs
tournent à vide. Mesuré : **14 min 41 s avec, 14 min 49 s sans**. Huit secondes
sur quinze minutes, c'est du bruit. Le crate de queue lui-même ne bouge pas
(95,4 s contre 96,2 s). Un drapeau `-Z` instable, qui invalide tout le cache le
jour où on l'ajoute, ne mérite pas d'être dans le dépôt pour ça.

**Baisser l'`opt-level` des dépendances.** `[profile.dev.package."*"]` compile
les ~600 dépendances en `opt-level = 3`. Descendre à 1 ferait gagner du temps,
mais la génération de monde passe son temps dans `vek`, `noise`, `hashbrown`,
`rayon` et `image` : on paierait en secondes de jeu ce qu'on gagnerait en
secondes de build. Le faire proprement demanderait de nommer crate par crate
les chauds et les froids, mesures de jeu à l'appui — ce n'est pas fait ici.

**sccache.** Utile pour repartir de zéro souvent (changement de branche,
`cargo clean`), inutile en incrémental — sccache ne met pas en cache les
compilations incrémentales.

**Le backend cranelift de rustc.** Le vrai gain sur les builds de debug, mais
`rustc_codegen_cranelift` n'est pas fiable sur `x86_64-pc-windows-msvc`.

**`--release` pour tester.** Le LTO sur tout le graphe est ce qui coûte le plus
cher, et n'apporte rien de visible ici : `no_overflow` est le bon profil pour
essayer une modification.
