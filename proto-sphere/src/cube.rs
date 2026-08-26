//! La topologie : le monde est le patron d'un cube (D27).
//!
//! Six faces carrées disposées en croix dans une grille plate. Les
//! coordonnées canoniques restent des coordonnées de grille — c'est ce qui
//! garde D27 vraie — mais tout déplacement passe par [`replier`], qui ramène
//! une position quelconque dans le patron et dit de combien de quarts de tour
//! le repère a tourné en chemin.
//!
//! Rien ici n'est écrit sous forme de vingt-quatre cas particuliers. Chaque
//! face porte une **base 3D entière** `(r, h, n)` — droite, haut, normale
//! sortante, avec `r × h = n`. Franchir un bord, c'est faire basculer cette
//! base autour de l'arête ; un seul chemin de code couvre les vingt-quatre
//! arêtes, et les invariants de `--diag` répondent de lui.
//!
//! Les recollements du cube **préservent l'orientation** : ce sont des
//! rotations, jamais des réflexions.

use crate::monde::TAILLE_CHUNK;

/// Arête d'une face, en chunks.
pub const FACE_CHUNKS: i32 = 96;
/// Arête d'une face, en blocs.
pub const FACE: i32 = FACE_CHUNKS * TAILLE_CHUNK;

/// Rayon de la planète, en blocs.
///
/// Il n'est pas libre. La projection conforme fixe le rapport entre un pas de
/// grille et un arc de sphère ; on choisit `R` pour qu'un bloc mesure un bloc
/// **au centre d'une face**, ce qui donne `R = arête/(4K)`. Ailleurs le bloc
/// est plus petit — c'est ce que coûte la conformité.
///
/// C'est le prix du rendu en vraie 3D : la rondeur n'est plus un réglage, c'est
/// la taille du monde.
pub const RAYON: f64 = FACE as f64 / (4.0 * crate::conforme::K);

pub const PATRON_COLS: i32 = 4;
pub const PATRON_LIGNES: i32 = 3;

/// Le patron, en blocs.
pub const NET_W: i32 = PATRON_COLS * FACE;
pub const NET_H: i32 = PATRON_LIGNES * FACE;

pub type V3 = [i32; 3];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Base {
    pub r: V3,
    pub h: V3,
    pub n: V3,
}

/// Les six faces. `+Z` et `−Z` sont les calottes polaires (D24) ; les quatre
/// autres forment la bande équatoriale, `h` pointant vers le nord.
pub const BASES: [Base; 6] = [
    Base { n: [1, 0, 0], r: [0, 1, 0], h: [0, 0, 1] },    // 0 · +X
    Base { n: [0, 1, 0], r: [-1, 0, 0], h: [0, 0, 1] },   // 1 · +Y
    Base { n: [-1, 0, 0], r: [0, -1, 0], h: [0, 0, 1] },  // 2 · −X
    Base { n: [0, -1, 0], r: [1, 0, 0], h: [0, 0, 1] },   // 3 · −Y
    Base { n: [0, 0, 1], r: [-1, 0, 0], h: [0, -1, 0] },  // 4 · +Z, calotte nord
    Base { n: [0, 0, -1], r: [-1, 0, 0], h: [0, 1, 0] },  // 5 · −Z, calotte sud
];

pub const NOMS: [&str; 6] = ["+X", "+Y", "−X", "−Y", "+Z nord", "−Z sud"];

/// Place de chaque face dans le patron : `(colonne, ligne)`, ligne 0 en bas.
///
/// ```text
///   ligne 2      ·      [ +Z ]     ·        ·
///   ligne 1   [ +X ]   [ +Y ]   [ −X ]   [ −Y ]
///   ligne 0      ·      [ −Z ]     ·        ·
/// ```
///
/// Cette disposition n'est pas décorative : elle est celle où toutes les
/// adjacences visibles du dessin sont de vraies adjacences du cube, sans
/// rotation. `--diag` le vérifie plutôt que de le croire.
pub const PATRON: [(i32, i32); 6] = [(0, 1), (1, 1), (2, 1), (3, 1), (1, 2), (1, 0)];

/// La face occupant une case du patron, s'il y en a une.
pub fn face_en(col: i32, ligne: i32) -> Option<u8> {
    PATRON
        .iter()
        .position(|&(c, l)| c == col && l == ligne)
        .map(|i| i as u8)
}

fn produit(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn oppose(v: V3) -> V3 { [-v[0], -v[1], -v[2]] }

fn face_de_normale(n: V3) -> u8 {
    BASES.iter().position(|b| b.n == n).expect("normale de face") as u8
}

// --------------------------------------------------------------------------
// Franchir un bord
// --------------------------------------------------------------------------

// Les quatre bascules. Dans chaque cas, l'axe de l'arête franchie est conservé
// et les deux autres pivotent d'un quart de tour autour de lui.

fn bord_r_plus(b: Base) -> Base { Base { r: oppose(b.n), h: b.h, n: b.r } }
fn bord_r_moins(b: Base) -> Base { Base { r: b.n, h: b.h, n: oppose(b.r) } }
fn bord_h_plus(b: Base) -> Base { Base { r: b.r, h: oppose(b.n), n: b.h } }
fn bord_h_moins(b: Base) -> Base { Base { r: b.r, h: b.n, n: oppose(b.h) } }

/// Nombre de quarts de tour `k` tels que `base` soit la base canonique de sa
/// face tournée `k` fois autour de sa normale.
fn quarts(base: Base) -> u8 {
    let canon = BASES[face_de_normale(base.n) as usize];
    let mut r = canon.r;
    for k in 0..4 {
        if r == base.r {
            return k;
        }
        r = produit(base.n, r);
    }
    unreachable!("deux bases orthonormées directes de même normale");
}

/// Exprime `(u, v)`, donné dans une base tournée de `k` quarts de tour, dans la
/// base canonique de la face.
///
/// Le calcul passe par des coordonnées centrées **doublées** : `f` est pair,
/// donc les centres de case tombent sur des demi-entiers, et doubler garde tout
/// exact.
fn tourner(u: i32, v: i32, k: u8, f: i32) -> (i32, i32) {
    let (a, b) = (2 * u - (f - 1), 2 * v - (f - 1));
    let (cos, sin) = COS_SIN[k as usize];
    let (a2, b2) = (a * cos - b * sin, a * sin + b * cos);
    ((a2 + f - 1) / 2, (b2 + f - 1) / 2)
}

pub const COS_SIN: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Ramène `(face, u, v)` dans le patron.
///
/// `u` et `v` peuvent être arbitrairement hors de `[0, f)` : ils décrivent
/// alors le **plan déroulé** autour de la face de départ, prolongé au-delà de
/// ses bords. C'est la seule façon dont le reste du programme voit le monde, et
/// c'est ce qui fait que la continuité aux coutures n'est pas un correctif.
///
/// Rend la position canonique et le nombre de quarts de tour subis.
///
/// **Près d'un coin, cette fonction n'est pas injective** — et c'est le fait
/// central de cette topologie. Trois faces s'y rejoignent, soit 270°, quand le
/// plan déroulé en offre 360° : les 90° en trop retombent forcément sur du
/// monde déjà vu. L'ordre de résolution ci-dessous décide lequel, rien de plus.
pub fn replier(face: u8, u: i32, v: i32, f: i32) -> (u8, i32, i32, u8) {
    let mut base = BASES[face as usize];
    let (mut u, mut v) = (u, v);

    for _ in 0..64 {
        if u >= f {
            u -= f;
            base = bord_r_plus(base);
        } else if u < 0 {
            u += f;
            base = bord_r_moins(base);
        } else if v >= f {
            v -= f;
            base = bord_h_plus(base);
        } else if v < 0 {
            v += f;
            base = bord_h_moins(base);
        } else {
            let k = quarts(base);
            let (uc, vc) = tourner(u, v, k, f);
            return (face_de_normale(base.n), uc, vc, k);
        }
    }
    unreachable!("le repliement converge");
}

pub fn replier_bloc(face: u8, u: i32, v: i32) -> (u8, i32, i32, u8) {
    replier(face, u, v, FACE)
}

/// Le repliement, pour une position **continue**.
///
/// Même chemin que [`replier_bloc`], plus la rotation de la part
/// fractionnaire autour du centre de sa case. C'est cette version qui rend le
/// franchissement d'un bord continu, et pas seulement exact case par case :
/// sans elle, tout ce qui échantillonne le voisinage — à commencer par le
/// repère de la caméra — se fait tronquer au bord de la face.
pub fn replier_continu(face: u8, u: f64, v: f64) -> (u8, f64, f64, u8) {
    let (bu, bv) = (u.floor() as i32, v.floor() as i32);
    let (fc, cu, cv, k) = replier_bloc(face, bu, bv);

    let (fu, fv) = (u - bu as f64 - 0.5, v - bv as f64 - 0.5);
    let (cos, sin) = COS_SIN[k as usize];
    let (cos, sin) = (cos as f64, sin as f64);
    (
        fc,
        cu as f64 + 0.5 + (fu * cos - fv * sin),
        cv as f64 + 0.5 + (fu * sin + fv * cos),
        k,
    )
}

/// La direction, pour une position de face quelconque — **hors de la face
/// aussi**, auquel cas elle est repliée d'abord.
pub fn direction_continue(face: u8, u: f64, v: f64) -> [f64; 3] {
    let (f, u, v, _) = replier_continu(face, u, v);
    direction(f, u, v)
}

pub fn replier_chunk(face: u8, cu: i32, cv: i32) -> (u8, i32, i32, u8) {
    replier(face, cu, cv, FACE_CHUNKS)
}

// --------------------------------------------------------------------------
// Patron ↔ face
// --------------------------------------------------------------------------

/// Position d'une case canonique dans le patron, en blocs.
pub fn vers_net(face: u8, u: i32, v: i32) -> (i32, i32) {
    let (col, ligne) = PATRON[face as usize];
    (col * FACE + u, ligne * FACE + v)
}

/// L'inverse. `None` sur un trou du patron — aucune position canonique n'y
/// tombe jamais.
pub fn depuis_net(x: i32, y: i32) -> Option<(u8, i32, i32)> {
    if x < 0 || y < 0 || x >= NET_W || y >= NET_H {
        return None;
    }
    let (col, ligne) = (x / FACE, y / FACE);
    face_en(col, ligne).map(|f| (f, x - col * FACE, y - ligne * FACE))
}

// --------------------------------------------------------------------------
// Le cube vers la sphère
// --------------------------------------------------------------------------

/// Point de la sphère unité correspondant au centre d'une case de face.
///
/// L'**ajustement tangent** (`tan(a·π/4)`) est ce qui empêche les cases de
/// s'entasser au centre des faces et de s'étirer vers les coins : il répartit
/// les cases à angle constant plutôt qu'à distance constante sur le plan
/// tangent. `--diag` chiffre ce qu'il reste de distorsion.
pub fn point_sphere(face: u8, u: i32, v: i32) -> [f64; 3] {
    // Le centre de la case `u` est à la coordonnée continue `u + 0,5`.
    direction(face, u as f64 + 0.5, v as f64 + 0.5)
}

/// La même chose pour une coordonnée de face continue. C'est cette fonction que
/// le vertex shader reproduit, à l'identique : elle est la seule définition de
/// la forme du monde.
///
/// Elle ne calcule rien : elle lit la table conforme (voir
/// [`crate::conforme`]), que le shader lit aussi, aux mêmes octets près.
pub fn direction(face: u8, u: f64, v: f64) -> [f64; 3] {
    let b = BASES[face as usize];
    let d = crate::conforme::table().direction_locale(
        2.0 * u / FACE as f64 - 1.0,
        2.0 * v / FACE as f64 - 1.0,
    );

    [
        b.r[0] as f64 * d[0] + b.h[0] as f64 * d[1] + b.n[0] as f64 * d[2],
        b.r[1] as f64 * d[0] + b.h[1] as f64 * d[1] + b.n[1] as f64 * d[2],
        b.r[2] as f64 * d[0] + b.h[2] as f64 * d[1] + b.n[2] as f64 * d[2],
    ]
}

/// L'inverse de [`direction`] : de quel endroit du monde vient une direction 3D.
///
/// C'est ce qui rend la projection **inversible**, et cette propriété n'est pas
/// un agrément : elle est ce qui permet à la visée de rester exacte. Un rayon
/// venu de l'écran est courbe par nature ; il est redressé ici, une fois, avant
/// que le monde ne soit interrogé — et le monde, lui, n'est jamais interrogé
/// qu'à plat.
pub fn depuis_direction(d: [f64; 3]) -> (u8, f64, f64) {
    let projete = |v: V3| d[0] * v[0] as f64 + d[1] * v[1] as f64 + d[2] * v[2] as f64;

    // La face est celle dont la normale domine.
    let mut face = 0u8;
    let mut meilleur = f64::MIN;
    for f in 0..6u8 {
        let dot = projete(BASES[f as usize].n);
        if dot > meilleur {
            meilleur = dot;
            face = f;
        }
    }

    let b = BASES[face as usize];
    let locale = [projete(b.r), projete(b.h), projete(b.n)];
    let (s, t) = crate::conforme::table().depuis_locale(locale);
    (
        face,
        (s + 1.0) * FACE as f64 / 2.0,
        (t + 1.0) * FACE as f64 / 2.0,
    )
}

/// Les huit coins du cube, identifiés par le triplet de signes de leur position.
/// Un même coin apparaît trois fois dans le patron : c'est ce que la carte 2D
/// doit rendre lisible.
pub fn coin_de(face: u8, cote_u: bool, cote_v: bool) -> u8 {
    let b = BASES[face as usize];
    let (su, sv) = (if cote_u { 1 } else { -1 }, if cote_v { 1 } else { -1 });
    let p = [
        b.n[0] + b.r[0] * su + b.h[0] * sv,
        b.n[1] + b.r[1] * su + b.h[1] * sv,
        b.n[2] + b.r[2] * su + b.h[2] * sv,
    ];
    let bit = |v: i32| if v > 0 { 1u8 } else { 0u8 };
    bit(p[0]) | (bit(p[1]) << 1) | (bit(p[2]) << 2)
}

/// La face voisine par un bord donné, et donc l'arête du cube que ce bord
/// matérialise. Sert à colorer les douze recollements sur la carte.
pub fn voisine(face: u8, bord: u8) -> u8 {
    let b = BASES[face as usize];
    let suivante = match bord {
        0 => bord_r_plus(b),
        1 => bord_r_moins(b),
        2 => bord_h_plus(b),
        _ => bord_h_moins(b),
    };
    face_de_normale(suivante.n)
}

/// Identifiant d'arête du cube : la paire non ordonnée des deux faces qu'elle
/// joint, ramenée à un indice de 0 à 11.
pub fn arete_de(face: u8, bord: u8) -> u8 {
    let autre = voisine(face, bord);
    let (a, b) = (face.min(autre), face.max(autre));
    let mut i = 0;
    for x in 0..6u8 {
        for y in (x + 1)..6u8 {
            if BASES[x as usize].n == oppose(BASES[y as usize].n) {
                continue; // faces opposées : pas d'arête commune
            }
            if x == a && y == b {
                return i;
            }
            i += 1;
        }
    }
    unreachable!("les deux faces partagent une arête");
}
