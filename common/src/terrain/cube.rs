//! La topologie : le monde est le patron d'un cube (D27).
//!
//! Six faces carrées disposées en croix dans la grille plate de Veloren. Les
//! coordonnées canoniques restent des `Vec2<i32>` — c'est ce qui garde intacts
//! `MapSizeLg`, l'indexation plate, le format de sauvegarde et les messages
//! réseau — mais tout déplacement passe par [`replier`], qui ramène une
//! position quelconque dans le patron et dit de combien de quarts de tour le
//! repère a tourné en chemin.
//!
//! ```text
//!   ligne 2      ·      [ +Z ]     ·        ·      calotte nord
//!   ligne 1   [ +X ]   [ +Y ]   [ −X ]   [ −Y ]    bande équatoriale
//!   ligne 0      ·      [ −Z ]     ·        ·      calotte sud
//!   ligne 3      ·        ·        ·        ·      (morte)
//! ```
//!
//! La croix occupe 6 des 16 emplacements d'une grille de 4 × 4 faces : dix
//! restent morts. C'est de la mémoire et du temps d'érosion gaspillés, pas une
//! faute de correction, et c'est le prix d'une **seule** définition de la forme
//! du monde — un stockage compact demanderait un second patron pour l'image de
//! carte.
//!
//! Rien ici n'est écrit sous forme de vingt-quatre cas particuliers. Chaque
//! face porte une **base 3D entière** `(r, h, n)` — droite, haut, normale
//! sortante, avec `r × h = n`. Franchir un bord, c'est faire basculer cette
//! base autour de l'arête ; un seul chemin de code couvre les vingt-quatre
//! arêtes, et les tests de ce module répondent de lui.
//!
//! Les recollements du cube **préservent l'orientation** : ce sont des
//! rotations, jamais des réflexions.

use super::{MapSizeLg, TERRAIN_CHUNK_BLOCKS_LG, conforme};
use vek::*;

/// Nombre de colonnes et de lignes de faces dans le patron.
///
/// Quatre lignes et non trois : la grille de simulation doit rester carrée et
/// en puissance de deux, la quatrième ligne est morte.
pub const PATRON_COTE: i32 = 4;

/// Place de chaque face dans le patron : `(colonne, ligne)`, ligne 0 en bas.
///
/// Cette disposition n'est pas décorative : elle est celle où toutes les
/// adjacences visibles du dessin sont de vraies adjacences du cube, sans
/// rotation. Les tests le vérifient plutôt que de le croire.
pub const PATRON: [(i32, i32); 6] = [(0, 1), (1, 1), (2, 1), (3, 1), (1, 2), (1, 0)];

pub const NOMS: [&str; 6] = ["+X", "+Y", "−X", "−Y", "+Z nord", "−Z sud"];

/// Les quatre quarts de tour, en cosinus et sinus entiers.
pub const COS_SIN: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

type V3 = [i32; 3];

/// Le repère entier d'une face : droite, haut, normale sortante, `r × h = n`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Base {
    pub r: V3,
    pub h: V3,
    pub n: V3,
}

/// Les six faces. `+Z` et `−Z` sont les calottes polaires (D24) ; les quatre
/// autres forment la bande équatoriale, `h` pointant vers le nord.
pub const BASES: [Base; 6] = [
    Base {
        n: [1, 0, 0],
        r: [0, 1, 0],
        h: [0, 0, 1],
    }, // 0 · +X
    Base {
        n: [0, 1, 0],
        r: [-1, 0, 0],
        h: [0, 0, 1],
    }, // 1 · +Y
    Base {
        n: [-1, 0, 0],
        r: [0, -1, 0],
        h: [0, 0, 1],
    }, // 2 · −X
    Base {
        n: [0, -1, 0],
        r: [1, 0, 0],
        h: [0, 0, 1],
    }, // 3 · −Y
    Base {
        n: [0, 0, 1],
        r: [-1, 0, 0],
        h: [0, -1, 0],
    }, // 4 · +Z, calotte nord
    Base {
        n: [0, 0, -1],
        r: [-1, 0, 0],
        h: [0, 1, 0],
    }, // 5 · −Z, calotte sud
];

// --------------------------------------------------------------------------
// Les tailles, déduites de la carte
// --------------------------------------------------------------------------

/// Logarithme de l'arête d'une face, en chunks.
///
/// Le patron fait quatre faces de côté, donc `face_lg = x_lg - 2`. Les deux
/// dimensions de la carte doivent être égales : c'est une invariante vérifiée
/// à la construction d'une `MapSizeLg` cubique.
#[inline(always)]
pub const fn face_lg(map: MapSizeLg) -> u32 { map.vec().x - 2 }

/// Arête d'une face, en chunks.
#[inline(always)]
pub const fn face_chunks(map: MapSizeLg) -> i32 { 1 << face_lg(map) }

/// Arête d'une face, en blocs.
#[inline(always)]
pub const fn face_blocs(map: MapSizeLg) -> i32 { 1 << (face_lg(map) + TERRAIN_CHUNK_BLOCKS_LG) }

/// Rayon de la planète, en blocs.
///
/// Il n'est pas libre. La projection conforme fixe le rapport entre un pas de
/// grille et un arc de sphère ; on choisit `R` pour qu'un bloc mesure un bloc
/// **au centre d'une face**, ce qui donne `R = arête/(4K)`. Ailleurs le bloc
/// est plus petit — c'est ce que coûte la conformité, et c'est pourquoi la
/// rondeur cesse d'être un réglage : vouloir un horizon lointain, c'est vouloir
/// un grand monde.
#[inline]
pub fn rayon(map: MapSizeLg) -> f64 { face_blocs(map) as f64 / (4.0 * conforme::K) }

// --------------------------------------------------------------------------
// Patron ↔ face
// --------------------------------------------------------------------------

/// La face occupant une case du patron, s'il y en a une.
#[inline]
pub fn face_en(col: i32, ligne: i32) -> Option<u8> {
    PATRON
        .iter()
        .position(|&(c, l)| c == col && l == ligne)
        .map(|i| i as u8)
}

/// Décompose une clé du patron en `(face, u, v)`, à l'échelle donnée.
///
/// `None` sur les dix emplacements morts et hors de la grille : aucune position
/// canonique n'y tombe jamais.
#[inline]
fn decomposer(cle: Vec2<i32>, f: i32) -> Option<(u8, i32, i32)> {
    if cle.x < 0 || cle.y < 0 || cle.x >= PATRON_COTE * f || cle.y >= PATRON_COTE * f {
        return None;
    }
    let (col, ligne) = (cle.x / f, cle.y / f);
    face_en(col, ligne).map(|face| (face, cle.x - col * f, cle.y - ligne * f))
}

/// L'inverse : la clé du patron pour une position canonique de face.
#[inline]
fn composer(face: u8, u: i32, v: i32, f: i32) -> Vec2<i32> {
    let (col, ligne) = PATRON[face as usize];
    Vec2::new(col * f + u, ligne * f + v)
}

/// La clé de chunk d'une position canonique de face.
#[inline]
pub fn cle_de_chunk(map: MapSizeLg, face: u8, cu: i32, cv: i32) -> Vec2<i32> {
    composer(face, cu, cv, face_chunks(map))
}

/// La position en blocs d'une position canonique de face.
#[inline]
pub fn cle_de_bloc(map: MapSizeLg, face: u8, u: i32, v: i32) -> Vec2<i32> {
    composer(face, u, v, face_blocs(map))
}

/// Décompose une clé de chunk en `(face, u, v)`.
#[inline]
pub fn chunk_en_face(map: MapSizeLg, cle: Vec2<i32>) -> Option<(u8, i32, i32)> {
    decomposer(cle, face_chunks(map))
}

/// La face à laquelle appartient une clé de chunk, s'il y en a une.
#[inline]
pub fn face_de_chunk(map: MapSizeLg, cle: Vec2<i32>) -> Option<u8> {
    decomposer(cle, face_chunks(map)).map(|(f, _, _)| f)
}

/// La face à laquelle appartient une position en blocs, s'il y en a une.
#[inline]
pub fn face_de_bloc(map: MapSizeLg, wpos: Vec2<i32>) -> Option<u8> {
    decomposer(wpos, face_blocs(map)).map(|(f, _, _)| f)
}

/// Une clé de chunk tombe-t-elle sur une des six faces ?
///
/// C'est le prédicat qui remplace `MapSizeLg::contains_chunk` partout où il
/// s'agissait de savoir si une case existe.
#[inline]
pub fn chunk_vivant(map: MapSizeLg, cle: Vec2<i32>) -> bool { face_de_chunk(map, cle).is_some() }

// --------------------------------------------------------------------------
// Franchir un bord
// --------------------------------------------------------------------------

fn produit(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn oppose(v: V3) -> V3 { [-v[0], -v[1], -v[2]] }

fn face_de_normale(n: V3) -> u8 {
    BASES
        .iter()
        .position(|b| b.n == n)
        .expect("normale de face") as u8
}

// Les quatre bascules. Dans chaque cas, l'axe de l'arête franchie est conservé
// et les deux autres pivotent d'un quart de tour autour de lui.

fn bord_r_plus(b: Base) -> Base {
    Base {
        r: oppose(b.n),
        h: b.h,
        n: b.r,
    }
}

fn bord_r_moins(b: Base) -> Base {
    Base {
        r: b.n,
        h: b.h,
        n: oppose(b.r),
    }
}

fn bord_h_plus(b: Base) -> Base {
    Base {
        r: b.r,
        h: oppose(b.n),
        n: b.h,
    }
}

fn bord_h_moins(b: Base) -> Base {
    Base {
        r: b.r,
        h: b.n,
        n: oppose(b.h),
    }
}

/// Nombre de quarts de tour `k` tels que `base` soit la base canonique de sa
/// face tournée `k` fois autour de sa normale.
///
/// Il se lit **sur la base**, il ne se compte pas en chemin : deux bases
/// orthonormées directes de même normale ne diffèrent que d'une rotation autour
/// d'elle.
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
/// exact en entiers.
///
/// Publique parce qu'elle sert aussi à faire tourner le **contenu** d'une case
/// : un chunk atteint par-delà une couture arrive tourné de `k` quarts de tour,
/// et ses blocs se rangent avec cette même formule à l'échelle `f = 32`.
pub fn tourner(u: i32, v: i32, k: u8, f: i32) -> (i32, i32) {
    let (a, b) = (2 * u - (f - 1), 2 * v - (f - 1));
    let (cos, sin) = COS_SIN[k as usize];
    let (a2, b2) = (a * cos - b * sin, a * sin + b * cos);
    ((a2 + f - 1) / 2, (b2 + f - 1) / 2)
}

/// Ramène `(face, u, v)` dans le patron, à l'échelle `f`.
///
/// `u` et `v` peuvent être arbitrairement hors de `[0, f)` : ils décrivent
/// alors le **plan déroulé** autour de la face de départ, prolongé au-delà de
/// ses bords. C'est la seule façon dont le reste du moteur doit voir le monde,
/// et c'est ce qui fait que la continuité aux coutures n'est pas un correctif.
///
/// **Près d'un coin, cette fonction n'est pas injective** — et c'est le fait
/// central de cette topologie. Trois faces s'y rejoignent, soit 270°, quand le
/// plan déroulé en offre 360° : les 90° en trop retombent forcément sur du
/// monde déjà vu. L'ordre de résolution ci-dessous décide lequel, rien de plus.
fn replier_brut(face: u8, u: i32, v: i32, f: i32) -> (u8, i32, i32, u8) {
    let mut base = BASES[face as usize];
    let (mut u, mut v) = (u, v);

    // Une itération ramène le point d'une face vers l'intérieur : la borne
    // couvre donc une trentaine de faces d'écart, très au-delà de ce qu'un pas
    // d'entité, une portée de vue ou un voisinage de génération demandent. Un
    // delta plus grand n'a pas de sens à replier — le plan déroulé cesse d'être
    // une description utile bien avant.
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

/// Le repliement, à l'échelle des blocs.
///
/// `ancre` est une position canonique du patron — elle fixe la face dans le
/// repère de laquelle `delta` est écrit. Rend la position canonique atteinte et
/// le nombre de quarts de tour subis, ou `None` si l'ancre ne tombe sur aucune
/// face.
#[inline]
pub fn replier(map: MapSizeLg, ancre: Vec2<i32>, delta: Vec2<i32>) -> Option<(Vec2<i32>, u8)> {
    let f = face_blocs(map);
    let (face, u, v) = decomposer(ancre, f)?;
    let (fc, uc, vc, k) = replier_brut(face, u + delta.x, v + delta.y, f);
    Some((composer(fc, uc, vc, f), k))
}

/// Le repliement, à l'échelle des chunks.
#[inline]
pub fn replier_chunk(
    map: MapSizeLg,
    ancre: Vec2<i32>,
    delta: Vec2<i32>,
) -> Option<(Vec2<i32>, u8)> {
    let f = face_chunks(map);
    let (face, u, v) = decomposer(ancre, f)?;
    let (fc, uc, vc, k) = replier_brut(face, u + delta.x, v + delta.y, f);
    Some((composer(fc, uc, vc, f), k))
}

/// Le voisin d'une clé de chunk dans une direction donnée.
///
/// C'est la fonction qui remplace le filtre de bornes de [`super::neighbors`] :
/// elle ne rend jamais de case morte, et `None` ne signale qu'une ancre morte.
///
/// **Elle peut rendre deux fois la même case.** Aux vingt-quatre cases de coin,
/// trois faces se rejoignent : les trois cases du coin forment un triangle et
/// non un carré, si bien qu'une des huit directions retombe sur une case déjà
/// atteinte par une autre. C'est le « sept voisins au lieu de huit » de D27, et
/// ce n'est pas cosmétique — un doublon compterait deux fois dans
/// l'accumulation de flux de l'érosion. La déduplication est faite par
/// [`super::neighbors`], qui est le seul endroit à devoir la connaître.
#[inline]
pub fn voisin(map: MapSizeLg, cle: Vec2<i32>, d: Vec2<i32>) -> Option<Vec2<i32>> {
    replier_chunk(map, cle, d).map(|(voisin, _)| voisin)
}

/// Une position continue du monde, **avec la face dans le repère de laquelle
/// elle est écrite**.
///
/// La face est portée, jamais redevinée. Ce n'est pas un confort : une position
/// repliée peut tomber exactement sur un bord de case, et la retrouver par un
/// `floor` désigne alors la case d'à côté — qui peut être hors de la face. La
/// face fait partie de la position, comme le cap fait partie du monde et non de
/// la grille (D27).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lieu {
    /// La face, de 0 à 5.
    pub face: u8,
    /// Coordonnées continues dans la face, en blocs, dans `[0, arête]`.
    pub u: f64,
    pub v: f64,
}

impl Lieu {
    /// La position dans le patron, en blocs. À n'employer que pour indexer :
    /// tout calcul de géométrie garde le [`Lieu`].
    #[inline]
    pub fn wpos(self, map: MapSizeLg) -> Vec2<f64> {
        let f = face_blocs(map);
        let (col, ligne) = PATRON[self.face as usize];
        Vec2::new(
            col as f64 * f as f64 + self.u,
            ligne as f64 * f as f64 + self.v,
        )
    }
}

/// Le lieu d'une position canonique du patron.
#[inline]
pub fn lieu_de(map: MapSizeLg, wpos: Vec2<f64>) -> Option<Lieu> {
    let f = face_blocs(map);
    let (face, u, v) = decomposer(wpos.map(|e| e.floor() as i32), f)?;
    Some(Lieu {
        face,
        u: u as f64 + wpos.x.fract_gt(),
        v: v as f64 + wpos.y.fract_gt(),
    })
}

/// Le repliement d'une position **continue**, en blocs.
///
/// Même chemin que [`replier`], plus la rotation de la part fractionnaire
/// autour du centre de sa case. C'est cette version qui rend le franchissement
/// d'un bord continu, et pas seulement exact case par case : sans elle, tout ce
/// qui échantillonne un voisinage — à commencer par le repère de la caméra — se
/// fait tronquer au bord de la face.
#[inline]
pub fn replier_continu(map: MapSizeLg, ancre: Vec2<i32>, delta: Vec2<f64>) -> Option<(Lieu, u8)> {
    let (face, u0, v0) = decomposer(ancre, face_blocs(map))?;
    Some(replier_lieu(
        map,
        face,
        u0 as f64 + delta.x,
        v0 as f64 + delta.y,
    ))
}

/// Le cœur du repliement continu, en coordonnées de face — le seul endroit où
/// la part fractionnaire tourne.
fn replier_lieu(map: MapSizeLg, face: u8, u: f64, v: f64) -> (Lieu, u8) {
    let f = face_blocs(map);
    let (bu, bv) = (u.floor() as i32, v.floor() as i32);
    let (fc, cu, cv, k) = replier_brut(face, bu, bv, f);

    let (fu, fv) = (u - bu as f64 - 0.5, v - bv as f64 - 0.5);
    let (cos, sin) = COS_SIN[k as usize];
    let (cos, sin) = (cos as f64, sin as f64);
    (
        Lieu {
            face: fc,
            u: cu as f64 + 0.5 + (fu * cos - fv * sin),
            v: cv as f64 + 0.5 + (fu * sin + fv * cos),
        },
        k,
    )
}

/// Le déplacement de grille menant d'une case à une case adjacente, écrit dans
/// le repère de `de`.
///
/// Remplace l'idiome `uniform_idx_as_vec2(a) - uniform_idx_as_vec2(b)`, qui
/// rend un écart énorme et faux dès que les deux cases sont de part et d'autre
/// d'une couture. `None` si les deux cases ne sont pas voisines.
///
/// Le parcours suit [`super::NEIGHBOR_DELTA`], et cet ordre est celui que
/// [`super::neighbors`] emploie pour dédupliquer : aux coins, où deux
/// directions mènent à la même case, les deux fonctions désignent donc la même.
pub fn delta(map: MapSizeLg, de: Vec2<i32>, vers: Vec2<i32>) -> Option<Vec2<i32>> {
    let f = face_chunks(map);
    let (face_de, u, v) = decomposer(de, f)?;
    // Le cas courant, et de très loin : les deux cases sont sur la même face, et
    // la différence des coordonnées est exacte. Il vaut d'être écrit à part —
    // la fonction est appelée dans la boucle intérieure de l'érosion, et le cas
    // général y coûterait huit repliements par case et par pas.
    if let Some((face_vers, u2, v2)) = decomposer(vers, f)
        && face_de == face_vers
    {
        let d = Vec2::new(u2 - u, v2 - v);
        if d != Vec2::zero() && d.map(|e| e.abs()).reduce_max() <= 1 {
            return Some(d);
        }
    }
    super::NEIGHBOR_DELTA
        .iter()
        .map(|&(x, y)| Vec2::new(x, y))
        .find(|&d| voisin(map, de, d) == Some(vers))
}

// --------------------------------------------------------------------------
// Le cube vers la sphère
// --------------------------------------------------------------------------

/// Direction unité du monde pour un lieu.
///
/// C'est cette fonction que le vertex shader devra reproduire à l'identique :
/// elle est la seule définition de la forme du monde. Elle ne calcule rien —
/// elle lit la table conforme, que le shader lira aussi, aux mêmes octets près.
pub fn direction_de(map: MapSizeLg, lieu: Lieu) -> Vec3<f64> {
    let f = face_blocs(map) as f64;
    let b = BASES[lieu.face as usize];
    let d = conforme::table().direction_locale(2.0 * lieu.u / f - 1.0, 2.0 * lieu.v / f - 1.0);
    Vec3::new(
        b.r[0] as f64 * d[0] + b.h[0] as f64 * d[1] + b.n[0] as f64 * d[2],
        b.r[1] as f64 * d[0] + b.h[1] as f64 * d[1] + b.n[1] as f64 * d[2],
        b.r[2] as f64 * d[0] + b.h[2] as f64 * d[1] + b.n[2] as f64 * d[2],
    )
}

/// La direction pour une position canonique du patron.
#[inline]
pub fn direction(map: MapSizeLg, wpos: Vec2<f64>) -> Option<Vec3<f64>> {
    lieu_de(map, wpos).map(|l| direction_de(map, l))
}

/// La direction, pour une position qui peut sortir de la face de son ancre :
/// elle est repliée d'abord, par la version continue.
#[inline]
pub fn direction_continue(map: MapSizeLg, ancre: Vec2<i32>, delta: Vec2<f64>) -> Option<Vec3<f64>> {
    replier_continu(map, ancre, delta).map(|(l, _)| direction_de(map, l))
}

/// L'inverse de [`direction`] : de quel endroit du monde vient une direction
/// 3D.
///
/// C'est ce qui rend la projection **inversible**, et cette propriété n'est pas
/// un agrément : elle est ce qui permet à la visée de rester exacte. Un rayon
/// venu de l'écran est courbe par nature ; il est redressé ici, une fois, avant
/// que le monde ne soit interrogé — et le monde, lui, n'est jamais interrogé
/// qu'à plat.
pub fn depuis_direction(map: MapSizeLg, d: Vec3<f64>) -> Lieu {
    let projete = |v: V3| d.x * v[0] as f64 + d.y * v[1] as f64 + d.z * v[2] as f64;

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
    let (s, t) = conforme::table().depuis_locale(locale);

    let f = face_blocs(map) as f64;
    Lieu {
        face,
        u: (s + 1.0) * f / 2.0,
        v: (t + 1.0) * f / 2.0,
    }
}

/// Le repère local en un lieu : la verticale, puis les deux tangentes du
/// paramétrage — **non orthogonalisées**.
///
/// Leur non-orthogonalité est le sujet, pas un défaut à corriger : près d'un
/// coin elles se coupent à 120°, et c'est exactement pourquoi un quart de tour
/// appliqué en coordonnées n'est pas un quart de tour dans le monde. Les demi-
/// différences passent par le repliement continu, donc elles franchissent le
/// bord de la face au lieu de s'y faire tronquer.
pub fn repere(map: MapSizeLg, lieu: Lieu) -> (Vec3<f64>, Vec3<f64>, Vec3<f64>) {
    let r = rayon(map);
    let echantillon = |du: f64, dv: f64| {
        let (l, _) = replier_lieu(map, lieu.face, lieu.u + du, lieu.v + dv);
        direction_de(map, l)
    };
    (
        direction_de(map, lieu),
        (echantillon(0.5, 0.0) - echantillon(-0.5, 0.0)) * r,
        (echantillon(0.0, 0.5) - echantillon(0.0, -0.5)) * r,
    )
}

/// Le franchissement d'une arête par une entité.
///
/// `depuis` est sa dernière position canonique, `vers` celle que la physique
/// vient de calculer — écrite dans le repère de `depuis`, et donc possiblement
/// hors de sa face. Rend le lieu canonique atteint et le nombre de quarts de
/// tour subis, ou `None` si rien n'a été franchi.
///
/// **À savoir — le pas doit rester court.** Le repliement résout `u` puis `v` :
/// un déplacement qui sort d'une face par deux bords à la fois franchit une
/// prolongation d'arête qui ne correspond à rien sur la surface, et le résultat
/// dépend alors de l'ordre du code plutôt que de la géométrie. À moins d'un
/// bloc par pas le cas ne peut pas se présenter ; au-delà, il faut
/// sous-découper.
pub fn franchir(map: MapSizeLg, depuis: Lieu, vers: Vec2<f64>) -> Option<(Lieu, u8)> {
    let f = face_blocs(map) as f64;
    let (col, ligne) = PATRON[depuis.face as usize];
    let locale = vers - Vec2::new(col as f64 * f, ligne as f64 * f);
    if (0.0..f).contains(&locale.x) && (0.0..f).contains(&locale.y) {
        return None;
    }
    Some(replier_lieu(map, depuis.face, locale.x, locale.y))
}

/// Transporte un déplacement de grille d'un lieu à un autre, en passant par le
/// monde.
///
/// C'est la seule façon juste de faire suivre un cap à travers une couture. Un
/// quart de tour appliqué aux coordonnées n'est exact que si les deux tangentes
/// sont perpendiculaires ; près d'un coin elles se coupent à 120°, et le banc
/// d'essai y a mesuré **25,6°** d'erreur. Ici le vecteur redevient un vecteur
/// du monde à l'aller, et se redécompose sur les tangentes — non
/// orthogonalisées — à l'arrivée.
pub fn transporter(map: MapSizeLg, de: Lieu, vers: Lieu, v: Vec2<f64>) -> Option<Vec2<f64>> {
    let (_, tu, tv) = repere(map, de);
    vers_coordonnees(map, vers, tu * v.x + tv * v.y)
}

/// **La transformation qui pose un objet plat sur la sphère** (D27).
///
/// Un objet petit devant le rayon ne subit de la projection qu'une
/// transformation **rigide** : sa flèche vaut `w²/8r`, soit 6·10⁻⁴ bloc pour un
/// objet de cinq blocs sur un rayon de cinq mille. C'est l'argument que D29
/// employait déjà pour la nappe du portail — localement, la sphère est plate,
/// et c'est tout ce qu'on lui demande ici.
///
/// D'où : rien à changer dans les shaders. Il suffit de composer la matrice de
/// modèle que chaque objet possède déjà — figure, sprite, particule — avec le
/// repère local de sa position, et de la placer au point projeté.
///
/// **Le repère est pris à l'origine de l'objet, jamais par sommet.** Chercher
/// la face sommet par sommet déchirerait en deux tout modèle à cheval sur une
/// couture : ses sommets se répartiraient entre deux bases. C'est la même
/// raison qui fait porter sa base à un chunk plutôt que de la lui faire
/// deviner.
///
/// `origine` est le point de convergence déjà projeté, celui que le rendu
/// retire à tout le monde. Rend `None` hors du patron.
pub fn pose(map: MapSizeLg, wpos: Vec3<f64>, origine: Vec3<f64>) -> Option<Mat4<f64>> {
    let lieu = lieu_de(map, wpos.xy())?;
    let (haut, _, tv) = repere(map, lieu);
    // Les tangentes ne sont pas orthogonales — près d'un coin elles se coupent à
    // 120°. Un objet a besoin d'un repère rigide, pas du paramétrage : on
    // orthonormalise, ce qui est légitime ici et ne l'est pas pour un cap.
    let nord = (tv - haut * tv.dot(haut)).normalized();
    let est = nord.cross(haut);
    let place = haut * (rayon(map) + wpos.z) - origine;

    Some(Mat4::new(
        est.x, nord.x, haut.x, place.x, //
        est.y, nord.y, haut.y, place.y, //
        est.z, nord.z, haut.z, place.z, //
        0.0, 0.0, 0.0, 1.0,
    ))
}

/// Décompose un déplacement du monde sur les deux tangentes du paramétrage.
///
/// Seul endroit où une intention 3D redevient un déplacement de grille. La
/// résolution est un système 2 × 2 sur la matrice de Gram : les tangentes ne
/// sont jamais parallèles — 60° au pire, à un coin — donc le déterminant reste
/// sain.
pub fn vers_coordonnees(map: MapSizeLg, lieu: Lieu, d: Vec3<f64>) -> Option<Vec2<f64>> {
    let (haut, tu, tv) = repere(map, lieu);
    let d = d - haut * d.dot(haut);

    let (guu, guv, gvv) = (tu.dot(tu), tu.dot(tv), tv.dot(tv));
    let det = guu * gvv - guv * guv;
    if det.abs() < f64::EPSILON {
        return None;
    }
    let (bu, bv) = (d.dot(tu), d.dot(tv));
    Some(Vec2::new(
        (bu * gvv - bv * guv) / det,
        (bv * guu - bu * guv) / det,
    ))
}

/// `x - floor(x)`, sans passer par `fract()` qui garde le signe.
trait FractPositive {
    fn fract_gt(self) -> f64;
}

impl FractPositive for f64 {
    #[inline(always)]
    fn fract_gt(self) -> f64 { self - self.floor() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        terrain::{NEIGHBOR_DELTA, TerrainChunkSize},
        vol::RectVolSize,
    };

    /// Une carte cubique menue : face de 16 chunks, soit 512 blocs d'arête. Les
    /// invariants ne dépendent pas de la taille, seul leur coût en dépend.
    fn carte() -> MapSizeLg {
        MapSizeLg::nouvelle_cubique(Vec2::new(6, 6)).expect("carte cubique valide")
    }

    fn tourner_v(d: Vec2<i32>, k: u8) -> Vec2<i32> {
        let (cos, sin) = COS_SIN[k as usize];
        Vec2::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
    }

    /// Les tailles se déduisent l'une de l'autre, et le patron tient dans la
    /// grille sans déborder.
    #[test]
    fn tailles_coherentes() {
        let map = carte();
        assert_eq!(face_chunks(map), 16);
        assert_eq!(face_blocs(map), 16 * TerrainChunkSize::RECT_SIZE.x as i32);
        assert_eq!(PATRON_COTE * face_chunks(map), map.chunks().x as i32);

        let vivantes = (0..map.chunks().y as i32)
            .flat_map(|y| (0..map.chunks().x as i32).map(move |x| Vec2::new(x, y)))
            .filter(|&c| chunk_vivant(map, c))
            .count();
        assert_eq!(vivantes, 6 * (face_chunks(map) * face_chunks(map)) as usize);
    }

    /// Replier une position canonique ne la bouge pas, et ne tourne rien.
    #[test]
    fn repliement_idempotent() {
        let map = carte();
        let f = face_chunks(map);
        for face in 0..6u8 {
            for cu in (0..f).step_by(3) {
                for cv in (0..f).step_by(5) {
                    let cle = cle_de_chunk(map, face, cu, cv);
                    assert_eq!(
                        replier_chunk(map, cle, Vec2::zero()),
                        Some((cle, 0)),
                        "face {face}, case ({cu}, {cv})"
                    );
                }
            }
        }
    }

    /// Sortir d'une face par un bord puis y revenir ramène exactement à la case
    /// de départ, et les deux rotations s'annulent.
    ///
    /// C'est l'invariant qui répond des vingt-quatre arêtes d'un coup : elles
    /// ne sont écrites nulle part, elles se déduisent des six bases.
    #[test]
    fn aller_retour_sur_toutes_les_aretes() {
        let map = carte();
        let f = face_chunks(map);
        let bords: [(Vec2<i32>, fn(i32, i32) -> (i32, i32)); 4] = [
            (Vec2::new(1, 0), |f, w| (f - 1, w)),
            (Vec2::new(-1, 0), |_, w| (0, w)),
            (Vec2::new(0, 1), |f, w| (w, f - 1)),
            (Vec2::new(0, -1), |_, w| (w, 0)),
        ];

        let mut testees = 0;
        for face in 0..6u8 {
            for (sortie, place) in bords {
                for w in 0..f {
                    let (cu, cv) = place(f, w);
                    let depart = cle_de_chunk(map, face, cu, cv);

                    let (dehors, k) = replier_chunk(map, depart, sortie).expect("case vivante");
                    assert!(chunk_vivant(map, dehors), "on sort sur une case morte");

                    let retour = tourner_v(-sortie, k);
                    let (revenu, k2) = replier_chunk(map, dehors, retour).expect("case vivante");

                    assert_eq!(revenu, depart, "face {face}, bord {sortie:?}, w = {w}");
                    assert_eq!(
                        (k + k2) % 4,
                        0,
                        "les deux rotations ne s'annulent pas : {k} puis {k2}"
                    );
                    testees += 1;
                }
            }
        }
        assert_eq!(testees, 4 * 6 * f);
    }

    /// Le repliement au bloc et le repliement au chunk disent la même chose.
    ///
    /// C'est cette cohérence qui permettra au rendu de placer un chunk atteint
    /// par-delà une couture, puis d'en tourner le contenu avec la même formule.
    #[test]
    fn accord_du_bloc_et_du_chunk() {
        let map = carte();
        let f = face_chunks(map);
        let taille = TerrainChunkSize::RECT_SIZE.x as i32;
        let locaux = [
            (0, 0),
            (taille - 1, 0),
            (0, taille - 1),
            (taille - 1, taille - 1),
            (7, 19),
        ];

        for face in 0..6u8 {
            let ancre = cle_de_chunk(map, face, 0, 0);
            for cu in -1..=f {
                for cv in -1..=f {
                    let dc = Vec2::new(cu, cv);
                    let (chunk, k) = replier_chunk(map, ancre, dc).expect("ancre vivante");

                    for (lu, lv) in locaux {
                        let ancre_bloc = ancre * taille + Vec2::new(lu, lv);
                        let (bloc, kb) =
                            replier(map, ancre_bloc, dc * taille).expect("ancre vivante");

                        assert_eq!(kb, k, "les deux échelles ne tournent pas pareil");
                        let (lu2, lv2) = tourner(lu, lv, k, taille);
                        assert_eq!(
                            bloc,
                            chunk * taille + Vec2::new(lu2, lv2),
                            "face {face}, chunk ({cu}, {cv}), local ({lu}, {lv})"
                        );
                    }
                }
            }
        }
    }

    /// Vingt-quatre cases ont sept voisins au lieu de huit, et ce sont
    /// exactement les coins de face. Partout ailleurs, huit.
    ///
    /// Le défaut est celui que D27 annonce ; ce qui compte est qu'il soit
    /// **borné et localisé**, pas qu'il disparaisse.
    #[test]
    fn les_coins_ont_sept_voisins() {
        let map = carte();
        let f = face_chunks(map);

        let distincts = |cle: Vec2<i32>| {
            let mut vus: Vec<Vec2<i32>> = Vec::with_capacity(8);
            for &(x, y) in NEIGHBOR_DELTA.iter() {
                let v = voisin(map, cle, Vec2::new(x, y)).expect("ancre vivante");
                assert!(chunk_vivant(map, v), "un voisin tombe sur une case morte");
                if !vus.contains(&v) {
                    vus.push(v);
                }
            }
            vus.len()
        };

        let mut defectueuses = 0;
        for face in 0..6u8 {
            for cu in 0..f {
                for cv in 0..f {
                    let cle = cle_de_chunk(map, face, cu, cv);
                    let n = distincts(cle);
                    let au_coin = (cu == 0 || cu == f - 1) && (cv == 0 || cv == f - 1);
                    if au_coin {
                        assert_eq!(n, 7, "coin ({cu}, {cv}) de la face {face}");
                        defectueuses += 1;
                    } else {
                        assert_eq!(n, 8, "case ({cu}, {cv}) de la face {face}");
                    }
                }
            }
        }
        assert_eq!(defectueuses, 24);
    }

    /// `delta` retrouve un déplacement qui mène d'une case à sa voisine, y
    /// compris à travers une couture, et n'en invente pas entre deux cases
    /// éloignées.
    #[test]
    fn delta_traverse_les_coutures() {
        let map = carte();
        let f = face_chunks(map);
        for face in 0..6u8 {
            for cu in [0, 1, f / 2, f - 2, f - 1] {
                for cv in [0, 1, f / 2, f - 2, f - 1] {
                    let cle = cle_de_chunk(map, face, cu, cv);
                    for &(x, y) in NEIGHBOR_DELTA.iter() {
                        let v = voisin(map, cle, Vec2::new(x, y)).expect("ancre vivante");
                        let retrouve = delta(map, cle, v).expect("cases voisines");
                        // Aux coins, deux directions mènent à la même case :
                        // on vérifie que celle rendue y mène, pas qu'elle soit
                        // celle qu'on avait prise.
                        assert_eq!(voisin(map, cle, retrouve), Some(v));
                    }
                    let loin = cle_de_chunk(map, (face + 3) % 6, f / 2, f / 2);
                    assert!(delta(map, cle, loin).is_none(), "voisinage inventé");
                }
            }
        }
    }

    /// La projection est inversible, et elle l'est **au millième de bloc**.
    ///
    /// Ce n'est pas un agrément : c'est ce qui permettra à la visée de
    /// redresser un rayon d'écran une fois, puis d'interroger le monde à
    /// plat.
    #[test]
    fn projection_inversible() {
        let map = carte();
        let f = face_blocs(map);
        let mut pire = 0.0f64;

        for face in 0..6u8 {
            for u in (0..f).step_by(37) {
                for v in (0..f).step_by(41) {
                    let lieu = Lieu {
                        face,
                        u: u as f64 + 0.5,
                        v: v as f64 + 0.5,
                    };
                    let retour = depuis_direction(map, direction_de(map, lieu));
                    // Une face différente au retour n'est pas un petit écart :
                    // on refuse de le noyer dans une moyenne.
                    assert_eq!(retour.face, face, "l'aller-retour change de face");
                    pire = pire.max((retour.wpos(map) - lieu.wpos(map)).magnitude());
                }
            }
        }
        assert!(pire < 1e-3, "aller-retour de la projection : {pire} bloc");
    }

    /// Le repère local existe partout, ses tangentes ne sont jamais parallèles,
    /// et un pas de grille reste borné — la contrepartie annoncée par D27.
    ///
    /// **La borne n'est pas celle que D27 annonçait.** Elle y disait « 0,69 au
    /// plus bas », chiffre pris sur un profil de cinq points le long de la
    /// diagonale d'une face. Or le minimum n'est pas sur la diagonale : il est
    /// sur l'**arête**, dans l'anneau de raccord, et il vaut **0,255**. Le
    /// balayage complet est `diag_balayage_du_raccord`, dans `conforme`. Une
    /// mesure de la mauvaise forme est pire que pas de mesure (D28) — celle-ci
    /// avait la mauvaise forme parce qu'elle échantillonnait une ligne au lieu
    /// d'une surface.
    ///
    /// La valeur du raccord n'est pas retouchée pour autant : l'élargir remonte
    /// le plancher mais triple la zone en losange, et ce compromis se juge à
    /// l'écran, pas sur un tableau. Il se rouvrira à l'étape du rendu, avec le
    /// balayage pour instrument.
    #[test]
    fn le_repere_local_tient_partout() {
        let map = carte();
        let f = face_blocs(map);
        let (mut court, mut long) = (f64::MAX, 0.0f64);
        let mut pire_angle = 180.0f64;

        for face in 0..6u8 {
            for u in (0..f).step_by(29) {
                for v in (0..f).step_by(31) {
                    let lieu = Lieu {
                        face,
                        u: u as f64 + 0.5,
                        v: v as f64 + 0.5,
                    };
                    let (haut, tu, tv) = repere(map, lieu);

                    assert!(
                        (haut.magnitude() - 1.0).abs() < 1e-9,
                        "la verticale n'est pas unitaire"
                    );
                    let angle = (tu.dot(tv) / (tu.magnitude() * tv.magnitude()))
                        .clamp(-1.0, 1.0)
                        .acos()
                        .to_degrees();
                    pire_angle = pire_angle.min(angle);
                    court = court.min(tu.magnitude()).min(tv.magnitude());
                    long = long.max(tu.magnitude()).max(tv.magnitude());
                }
            }
        }

        // Jamais parallèles : c'est ce qui garde sain le déterminant de
        // `vers_coordonnees`.
        assert!(pire_angle > 45.0, "tangentes trop proches : {pire_angle}°");
        assert!(court > 0.25, "un pas de grille tombe à {court} bloc");
        assert!(long < 1.01, "un pas de grille monte à {long} bloc");
    }

    /// **Le franchissement se mesure en marchant.**
    ///
    /// Poser deux repères de part et d'autre d'un bord et comparer leurs caps
    /// ne mesure que la continuité de la projection ; le transport, lui, ne
    /// se voit qu'en franchissant. C'est la leçon que le banc d'essai a
    /// payée le plus cher : une mesure de cette forme-là avait *certifié*
    /// continu un franchissement qui faisait sauter la visée de 25,6°.
    ///
    /// On marche donc pas à pas, un bloc à la fois, sur les vingt-quatre arêtes
    /// et à plusieurs distances des coins, et on relève la rotation du cap
    /// entre deux pas. Le cap est un **vecteur du monde**, redressé contre
    /// la verticale locale à chaque pas — le franchissement n'a rien à lui
    /// faire, et c'est justement ce qu'on vérifie.
    #[test]
    fn marcher_a_travers_les_bords() {
        // Une face plus grande : le plancher géométrique vaut `1 / rayon`, donc
        // une mesure prise sur un monde minuscule ne distingue rien.
        let map = MapSizeLg::nouvelle_cubique(Vec2::new(10, 10)).expect("carte cubique");
        let f = face_blocs(map);
        let r = rayon(map);
        let plancher = (1.0f64 / r).to_degrees();

        let bords: [(Vec2<f64>, fn(f64, f64) -> (f64, f64)); 4] = [
            (Vec2::new(1.0, 0.0), |f, w| (f - 0.5, w)),
            (Vec2::new(-1.0, 0.0), |_, w| (0.5, w)),
            (Vec2::new(0.0, 1.0), |f, w| (w, f - 0.5)),
            (Vec2::new(0.0, -1.0), |_, w| (w, 0.5)),
        ];

        let (mut pire, mut ou) = (0.0f64, String::new());
        let mut pas_total = 0u32;

        for face in 0..6u8 {
            for (sortie, place) in bords {
                // À plusieurs distances du coin : c'est près de lui que les
                // tangentes se coupent à 120° et qu'un quart de tour appliqué
                // en coordonnées se trompe.
                for coin in [2.0, 12.0, 60.0] {
                    for biais in [0.0, 0.7, -0.7] {
                        // On part en retrait, cap vers le bord, et on le franchit.
                        let (u, v) = place(f as f64, coin);
                        let depart = Vec2::new(u, v) - sortie * 7.0;
                        let mut lieu = Lieu {
                            face,
                            u: depart.x,
                            v: depart.y,
                        };

                        // Le cap, en vecteur du monde dès le départ.
                        let (_, tu, tv) = repere(map, lieu);
                        let vise = sortie + Vec2::new(-sortie.y, sortie.x) * biais;
                        let mut cap = (tu * vise.x + tv * vise.y).normalized();

                        for _ in 0..14 {
                            let (haut, _, _) = repere(map, lieu);
                            let redresse = (cap - haut * cap.dot(haut)).normalized();
                            let rotation = cap.dot(redresse).clamp(-1.0, 1.0).acos().to_degrees();
                            if rotation > pire {
                                pire = rotation;
                                ou = format!(
                                    "face {}, bord {sortie:?}, à {coin} du coin",
                                    NOMS[face as usize]
                                );
                            }

                            let Some(d) = vers_coordonnees(map, lieu, redresse) else {
                                break;
                            };
                            let (suivant, _) =
                                replier_lieu(map, lieu.face, lieu.u + d.x, lieu.v + d.y);
                            lieu = suivant;
                            cap = redresse;
                            pas_total += 1;
                        }
                    }
                }
            }
        }

        println!(
            "rotation du cap : {pire:.4}°/bloc · plancher géométrique {plancher:.4}° ·              {pas_total} pas · pire en {ou}"
        );
        // Le plancher n'est pas zéro : marcher un bloc sur une sphère de rayon
        // `r` fait tourner le cap de `1/r`. On accepte le double, pas plus.
        assert!(
            pire < 2.0 * plancher,
            "rotation du cap : {pire:.4}°/bloc pour un plancher de {plancher:.4}° ({ou}, \
             {pas_total} pas)"
        );
    }

    /// Le franchissement ne se déclenche que lorsqu'on sort, et il rend une
    /// position canonique.
    #[test]
    fn franchir_ne_bouge_rien_a_l_interieur() {
        let map = carte();
        let f = face_blocs(map) as f64;
        for face in 0..6u8 {
            let lieu = Lieu {
                face,
                u: f / 2.0,
                v: f / 2.0,
            };
            assert!(
                franchir(map, lieu, lieu.wpos(map)).is_none(),
                "franchissement déclenché sans sortie"
            );
            // Un pas qui sort : la position rendue est canonique.
            let dehors = lieu.wpos(map) + Vec2::new(f, 0.0);
            let (arrivee, _) = franchir(map, lieu, dehors).expect("sortie franchie");
            assert!(
                (0.0..f).contains(&arrivee.u) && (0.0..f).contains(&arrivee.v),
                "la position rendue n'est pas canonique"
            );
        }
    }

    /// Un déplacement transporté d'une case à sa voisine par-delà une couture,
    /// puis ramené, revient sur lui-même.
    ///
    /// C'est la propriété que le quart de tour appliqué en coordonnées **n'a
    /// pas** près d'un coin, où les tangentes se coupent à 120°.
    #[test]
    fn transport_reversible() {
        let map = carte();
        let f = face_blocs(map);
        let mut pire = 0.0f64;

        for face in 0..6u8 {
            for (u, v) in [
                (f - 1, f / 2),
                (f / 2, f - 1),
                (f - 1, f - 1),
                (f - 2, f - 2),
            ] {
                let depart = Lieu {
                    face,
                    u: u as f64 + 0.5,
                    v: v as f64 + 0.5,
                };
                let (arrivee, _) = replier_lieu(map, face, depart.u + 1.5, depart.v + 1.5);

                for v0 in [
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(0.7, -0.7),
                ] {
                    let la = transporter(map, depart, arrivee, v0).expect("repères valides");
                    let retour = transporter(map, arrivee, depart, la).expect("repères valides");
                    pire = pire.max((retour - v0).magnitude());
                }
            }
        }
        // Le plancher n'est pas libre : les tangentes sont des différences
        // finies sur une table en `f32`, divisées par un pas d'un bloc puis
        // multipliées par le rayon. Le bruit de la table, ~1e-7, ressort donc
        // multiplié par `rayon / 1 bloc`. Il croît avec la taille du monde, et
        // c'est le prix du partage octet pour octet avec le shader — garder du
        // `f64` ici achèterait de la précision contre une divergence.
        assert!(
            pire < 1e-3,
            "aller-retour du transport : {pire} pas de grille"
        );
    }
}
