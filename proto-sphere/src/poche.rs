//! La poche : le second monde, et sa caméra.
//!
//! Une salle bornée, plate, finie. C'est le passé d'une ruine réduit à sa
//! plomberie (D8) : un lieu séparé, ouvert à la demande, refermé derrière soi.
//!
//! Ce fichier ne connaît **ni [`crate::cube`], ni [`crate::monde::Generateur`]**,
//! et c'est délibéré. D17 affirme que le coût d'un second monde est
//! architectural et non volumétrique ; la seule façon de le mettre à l'épreuve
//! est de rendre la disjonction structurelle. Aucune fonction d'ici ne prend de
//! `&Generateur` en argument, si bien qu'une fuite de la sphère vers la poche
//! ne peut pas s'écrire — pas seulement ne pas arriver.
//!
//! Le mur d'enceinte n'est pas un décor : c'est la borne de D13, et il
//! l'annonce à l'œil. Une limite de déplacement invisible est un mur invisible,
//! ce qui est exactement ce que D13 interdit.

use crate::monde::{Bloc, HAUTEUR_CHUNK, TAILLE_CHUNK};
use glam::Vec3;

/// Six chunks de côté : 192 blocs. Assez pour marcher, assez petit pour voir
/// les quatre murs d'un coup d'œil — la finitude doit se lire, pas se déduire.
pub const COTE_CHUNKS: i32 = 6;
pub const COTE: i32 = COTE_CHUNKS * TAILLE_CHUNK;

/// Altitude du dallage, et hauteur du mur au-dessus.
pub const DALLAGE: i32 = 8;
pub const MUR: i32 = 26;

/// Épaisseur du mur d'enceinte, en blocs.
pub const EPAISSEUR: i32 = 4;

/// Le portail de sortie : là où l'on débouche, et ce que la nappe montre.
///
/// Il est **adossé au mur sud**, tourné vers la salle. Son repère est celui des
/// axes : droite `+X`, avant `+Y`, haut `+Z`. C'est ce qui rend la
/// transformation depuis la sphère si courte — d'un côté un repère quelconque
/// sur une sphère, de l'autre l'identité.
pub const SORTIE_U: f32 = COTE as f32 * 0.5;
pub const SORTIE_V: f32 = (EPAISSEUR + 1) as f32;
pub const SORTIE_Z: f32 = (DALLAGE + 6) as f32;

/// Le centre de sa nappe, d'un seul tenant.
pub const SORTIE: Vec3 = Vec3::new(SORTIE_U, SORTIE_V, SORTIE_Z);

/// Le pied de son cadre : le maillage est bâti depuis le sol du cadre, et son
/// centre se trouve [`crate::ancre::CENTRE_NAPPE`] blocs plus haut.
pub const SORTIE_PIED: f32 = SORTIE_Z - crate::ancre::CENTRE_NAPPE;

/// Le pas passe-t-il à travers le portail de sortie ?
///
/// Le pendant exact de [`crate::ancre::Portail::franchi`], en plus simple :
/// ici le plan est `v = SORTIE_V` et le rectangle est aligné sur les axes. Les
/// deux sens comptent — on entre par là, on ressort par là.
pub fn franchi_sortie(avant: Vec3, apres: Vec3) -> Option<f32> {
    let (d0, d1) = (avant.y - SORTIE_V, apres.y - SORTIE_V);
    if d0 == d1 || (d0 > 0.0) == (d1 > 0.0) {
        return None;
    }
    let t = d0 / (d0 - d1);
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let p = avant + (apres - avant) * t;
    let demi = crate::ancre::LARGEUR_NAPPE * 0.5;
    if (p.x - SORTIE_U).abs() > demi
        || (p.z - SORTIE_Z).abs() > crate::ancre::HAUTEUR_NAPPE * 0.5
    {
        return None;
    }
    Some(t)
}

/// Le plan de coupe du portail de sortie, en `(normale, distance)`.
///
/// Ce qui est derrière lui — le mur, et tout le dehors — ne doit pas se voir à
/// travers la nappe. La caméra virtuelle se trouvant *derrière* le portail,
/// c'est le mur qu'elle aurait dans le nez sans cette coupe.
pub const COUPE_SORTIE: [f32; 4] = [0.0, 1.0, 0.0, -SORTIE_V];

/// Le ciel du passé. Il diffère de [`crate::vue3d::CIEL`] pour que la bascule
/// soit immédiatement lisible : on ne se demande jamais dans quel monde on est.
pub const CIEL_POCHE: [f32; 4] = [0.09, 0.06, 0.15, 1.0];

/// Un mélange d'entiers, sans dépendance : la poche ne veut pas de bruit
/// fractal, elle veut de l'architecture, donc des décisions nettes.
fn brouiller(x: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h
}

pub struct Poche {
    /// L'instance. Elle entre dans la clé de cache des mailles, si bien qu'une
    /// fenêtre rouverte ne réutilise jamais les mailles de la précédente —
    /// D9 : la fenêtre refermée efface ce qu'on y a bâti.
    pub graine: u32,
}

impl Poche {
    pub fn nouvelle(graine: u32) -> Self {
        Self { graine }
    }

    /// Dans le mur d'enceinte ?
    fn enceinte(u: i32, v: i32) -> bool {
        u < EPAISSEUR || v < EPAISSEUR || u >= COTE - EPAISSEUR || v >= COTE - EPAISSEUR
    }

    /// Hauteur du pilier posé sur cette case, ou zéro s'il n'y en a pas.
    ///
    /// Les piliers ne sont pas décoratifs non plus : sans repère vertical, un
    /// dallage plat ne dit rien du déplacement, et on ne pourrait pas juger si
    /// le rendu plat l'est vraiment.
    fn pilier(&self, u: i32, v: i32) -> i32 {
        const MAILLE: i32 = 38;
        const LARGEUR: i32 = 5;

        let (cu, cv) = (u.div_euclid(MAILLE), v.div_euclid(MAILLE));
        let h = brouiller(
            self.graine.wrapping_mul(0x0F1B)
                ^ (cu as u32).wrapping_mul(73_856_093)
                ^ (cv as u32).wrapping_mul(19_349_663),
        );
        // Une case de la maille sur trois porte un pilier.
        if !h.is_multiple_of(3) {
            return 0;
        }
        let (lu, lv) = (u.rem_euclid(MAILLE), v.rem_euclid(MAILLE));
        if lu >= LARGEUR || lv >= LARGEUR {
            return 0;
        }
        8 + ((h >> 8) % 12) as i32
    }

    /// Hauteur du piédestal central, ou zéro.
    fn piedestal(u: i32, v: i32) -> i32 {
        let du = (u - COTE / 2).abs();
        let dv = (v - COTE / 2).abs();
        match du.max(dv) {
            0..=5 => 4,
            6..=8 => 3,
            9..=11 => 2,
            12..=14 => 1,
            _ => 0,
        }
    }

    /// Le bloc en `(u, v, z)`.
    ///
    /// **Hors des bornes, c'est de l'air, toujours.** La poche est finie ; rien
    /// ne la prolonge, et surtout pas la sphère.
    pub fn bloc(&self, u: i32, v: i32, z: i32) -> Bloc {
        if u < 0 || v < 0 || u >= COTE || v >= COTE || !(0..HAUTEUR_CHUNK).contains(&z) {
            return Bloc::Air;
        }

        if z < DALLAGE {
            return Bloc::Roche;
        }
        if z == DALLAGE {
            // Un damier de huit blocs : c'est lui qui donne l'échelle, et donc
            // ce qui permet de juger à l'œil que le sol est bien plat.
            let sombre = (u.div_euclid(8) + v.div_euclid(8)).rem_euclid(2) == 0;
            return if sombre { Bloc::Roche } else { Bloc::Sable };
        }

        let sol = z - DALLAGE;
        if Self::enceinte(u, v) {
            return if sol <= MUR { Bloc::Roche } else { Bloc::Air };
        }
        if sol <= self.pilier(u, v) {
            return Bloc::Terre;
        }
        if sol <= Self::piedestal(u, v) {
            return Bloc::Glace;
        }
        Bloc::Air
    }

    /// Le sommet atteint par la salle : borne la boucle de maillage.
    pub fn plafond() -> i32 {
        DALLAGE + MUR + 2
    }

    /// Où l'on arrive en franchissant le portail : **juste devant le portail de
    /// sortie**, tourné vers la salle.
    ///
    /// Ce n'est pas un détail de mise en scène. Depuis que la nappe montre le
    /// passé en direct, ce qu'on voit à travers et ce qu'on obtient en entrant
    /// doivent être la même chose : arriver ailleurs que là où le regard était
    /// posé ferait mentir la fenêtre.
    ///
    /// Le regard est **horizontal**, et ce n'est pas cosmétique non plus : la
    /// caméra vole et avance dans la direction où elle regarde. Un tangage même
    /// léger la fait descendre à chaque pas, et au bout d'une quarantaine de pas
    /// elle rase le dallage — on ne voit plus la salle, on voit le sol.
    pub fn depart(&self) -> CameraPlate {
        CameraPlate {
            position: Vec3::new(SORTIE_U, SORTIE_V + 1.0, SORTIE_Z),
            regard: Vec3::Y,
            tangage: 0.0,
        }
    }
}

// --------------------------------------------------------------------------
// La caméra de la poche
// --------------------------------------------------------------------------

/// La caméra plate.
///
/// Volontairement **séparée** de [`crate::vue3d::Camera`], qui reste la caméra
/// de la sphère et rien d'autre. Cette dernière est l'objet que les trois
/// règles du banc surveillent ; y glisser un `if poche` mettrait le drapeau
/// exactement là où il ferait le plus de dégâts. Ici il n'y a ni face, ni
/// repliement, ni projection : le monde est le plan, la verticale est `+Z`.
#[derive(Clone, Copy)]
pub struct CameraPlate {
    pub position: Vec3,
    /// Horizontal, unitaire. Comme sur la sphère, c'est un vecteur du monde —
    /// sauf qu'ici le monde et la grille coïncident, ce qui est justement la
    /// différence que le banc est là pour montrer.
    pub regard: Vec3,
    pub tangage: f32,
}

impl CameraPlate {
    /// Position, direction de visée, verticale. La verticale ne dépend de rien.
    pub fn repere(&self) -> (Vec3, Vec3, Vec3) {
        let (st, ct) = self.tangage.sin_cos();
        let avant = (self.regard * ct + Vec3::Z * st).normalize();
        (self.position, avant, Vec3::Z)
    }

    pub fn droite(&self) -> Vec3 {
        self.regard.cross(Vec3::Z).normalize()
    }

    pub fn tourner(&mut self, angle: f32) {
        let (s, c) = angle.sin_cos();
        self.regard = Vec3::new(
            self.regard.x * c - self.regard.y * s,
            self.regard.x * s + self.regard.y * c,
            0.0,
        )
        .normalize();
    }

    /// Avance, et se borne à l'intérieur de l'enceinte.
    ///
    /// La borne est celle de D13, et le mur la rend lisible : on ne s'arrête
    /// pas contre un mur invisible, on s'arrête contre un mur.
    pub fn avancer(&mut self, deplacement: Vec3) {
        // La borne s'arrête juste devant le mur — mais pas devant le portail :
        // il faut pouvoir revenir sur ses pas et repasser par où l'on est
        // entré. C'est le franchissement qui décide, avant que la borne ne
        // s'applique.
        let bord = (EPAISSEUR + 1) as f32 - 0.5;
        self.position += deplacement;
        self.position.x = self.position.x.clamp(bord, COTE as f32 - bord);
        self.position.y = self.position.y.clamp(bord, COTE as f32 - bord);
        // Trois blocs au-dessus du dallage au minimum : plus bas, le plan
        // rapproché entre dans le sol et la salle cesse de se lire.
        self.position.z = self
            .position
            .z
            .clamp((DALLAGE + 3) as f32, (DALLAGE + MUR + 8) as f32);
    }
}
