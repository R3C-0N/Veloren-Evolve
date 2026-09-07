# Temps de compilation

Un build complet de voxygen part d'environ 700 crates et se termine par les
trois gros morceaux de la maison — `world`, `common`, `voxygen` — puis par une
édition de liens sur un exécutable de plusieurs centaines de mégaoctets. Trois
postes, donc, et trois familles de remèdes ; ce fichier dit lesquels sont déjà
en place dans le dépôt et lesquels demandent un geste de plus.

Tout ce qui suit vise le **build de développement**. Les builds de publication
(`--release`, avec LTO sur tout le graphe) sont lents par construction et c'est
très bien ainsi.

Les chiffres de crates ci-dessous sont comptés sur le graphe réel
(`cargo tree -e normal,build --target x86_64-pc-windows-msvc`), dédoublonnés par
crate et version. Ils mesurent une quantité de travail, pas des minutes : la
conversion dépend de la machine, mais l'ordre des leviers, lui, ne bouge pas.

## 1. Compiler moins : `cargo fast-voxygen`

Les features par défaut de voxygen tirent **690 crates** sur Windows. Elles
incluent `plugins`, qui embarque **wasmtime et cranelift** — un compilateur JIT
complet, 95 crates à lui seul, parmi les plus lents de l'écosystème — alors que
le dépôt ne livre aucun greffon (`plugin/` ne contient que des exemples et les
définitions `wit`). Elles incluent aussi `discord` et `native-dialog`, ce
dernier ne servant qu'à afficher une boîte de dialogue quand le jeu panique.

    cargo fast-voxygen          # build
    cargo fast-run              # build + lancement
    cargo fast-run --partie-rapide

L'alias est défini dans `.cargo/config.toml` : profil `no_overflow`, et
`--no-default-features --features singleplayer,simd,hot-reloading,shaderc-from-source,egui-ui`.
Soit **586 crates au lieu de 690**, pour un client qui se joue exactement
pareil en solo.

`egui-ui` est gardé exprès : il ne coûte que treize crates, et le pilote de la
compétence `run-veloren-evolve` attend la ligne de journal `egui_wgpu` pour
savoir que le démarrage est fini. Le retirer casserait `driver.ps1 -Action
launch`.

**Ne pas alterner** entre `cargo build` (features par défaut) et
`cargo fast-voxygen` : chaque changement de jeu de features force la
recompilation des crates concernés. Choisir un des deux et s'y tenir.

## 2. Relier plus vite : `rust-lld` sur Windows

`link.exe`, l'éditeur de liens de MSVC, est le poste dominant du build
**incrémental** : après une ligne changée dans voxygen, `cargo build` ne fait
presque plus que relier. `rust-lld` est livré avec le toolchain Rust — il est
donc déjà installé — et fait le même travail plusieurs fois plus vite.

C'est fait, dans `.cargo/config.toml` :

```toml
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```

Sur Linux, le dépôt utilisait déjà `mold` de la même façon.

## 3. Occuper tous les cœurs : `-Z threads=8`

À la fin d'un build complet, il ne reste que `world`, puis `common`, puis
`voxygen` : des crates énormes que rustc compilait sur **un seul thread**
pendant que les autres cœurs tournaient à vide. Le front-end parallèle découpe
l'analyse d'un même crate sur plusieurs threads.

C'est ajouté aux `rustflags` de chaque cible dans `.cargo/config.toml`. Deux
choses à savoir :

- les `rustflags` d'une section `[target.*]` **remplacent** ceux de `[build]`
  au lieu de s'y ajouter — d'où la répétition du drapeau dans chaque cible ;
- c'est un drapeau `-Z`, donc nightly seulement. Le toolchain est épinglé
  (`rust-toolchain`), donc c'est sans risque de dérive ; mais **en cas d'ICE du
  compilateur, c'est la première chose à retirer**.

Ajouter un `rustflag` invalide le cache : le premier build après ce changement
est un build complet.

## 4. Ne pas compiler shaderc : le SDK Vulkan

La feature `shaderc-from-source` compile **glslang et SPIRV-Tools depuis leurs
sources C++**, avec cmake, Python et ninja en prérequis. Le SDK Vulkan livre
exactement la même bibliothèque, déjà construite (`shaderc_combined.lib`).

Si le SDK Vulkan est installé, `shaderc-sys` le trouve tout seul par la
variable `VULKAN_SDK` : il suffit de **retirer `shaderc-from-source`** des deux
alias `fast-voxygen` et `fast-run`. On peut aussi pointer un autre répertoire
avec `SHADERC_LIB_DIR`.

Ce n'est payant qu'au premier build — ensuite l'artefact est en cache dans
`target/` — mais c'est aussi trois prérequis de moins à installer.

## 5. Travailler sur `world` sans compiler voxygen

C'est le plus gros levier pour qui touche à la génération de monde, et il ne
demande aucune configuration : **`veloren-world` ne dépend que de 206 crates**,
contre 586 pour le client. Ni wgpu, ni shaderc, ni conrod, ni iced, ni
l'édition de liens du gros exécutable.

    cargo run --profile no_overflow -p veloren-world --example empreinte
    cargo run --profile no_overflow -p veloren-world --example reglages
    cargo run --profile no_overflow -p veloren-world --example view

`empreinte` est déjà l'oracle de non-régression du terrain ; `view` affiche la
carte. Une boucle de travail qui reste dans `world` et ne lance le client que
pour vérifier le rendu final coûte une fraction du temps.

Même logique pour `cargo check` : il ne fait pas de codegen et répond en
quelques dizaines de secondes là où `cargo build` prend des minutes. Le
compiler-error loop se fait sur `cargo check`, pas sur `cargo build`.

## 6. Recharger à chaud plutôt que recompiler

Deux features existent déjà pour éviter un cycle complet :

- `hot-anim` (`anim/use-dyn-lib`) recompile les animations en bibliothèque
  dynamique, sans relier voxygen ;
- `hot-reloading` recharge les assets sans redémarrer — elle est dans le jeu de
  features de `fast-voxygen`.

## Ce qui n'a pas été retenu

**Baisser l'`opt-level` des dépendances.** `[profile.dev.package."*"]` compile
les ~600 dépendances en `opt-level = 3`. Descendre à 1 ferait gagner du temps,
mais la génération de monde passe son temps dans `vek`, `noise`, `hashbrown`,
`rayon` et `image` : on paierait en secondes de jeu ce qu'on gagnerait en
secondes de build. Le faire proprement demanderait de nommer crate par crate
les chauds et les froids, mesures à l'appui — ce n'est pas fait ici.

**sccache.** Utile pour repartir de zéro souvent (changement de branche,
`cargo clean`), inutile en incrémental — sccache ne met pas en cache les
compilations incrémentales.

**Le backend cranelift de rustc.** Le vrai gain sur les builds de debug, mais
`rustc_codegen_cranelift` n'est pas fiable sur `x86_64-pc-windows-msvc`.

**`--release` pour tester.** Le LTO sur tout le graphe est ce qui coûte le plus
cher, et n'apporte rien de visible ici : `no_overflow` est le bon profil pour
essayer une modification.
