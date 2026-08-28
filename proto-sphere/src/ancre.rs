//! L'ancre temporelle : la porte, le point de retour, et la fenêtre.
//!
//! D13 fait servir une seule idée à trois fonctions. Ici on n'en garde que la
//! plomberie : ouvrir un passage vers un second monde, y basculer en le
//! franchissant, et revenir **exactement** d'où l'on vient.
//!
//! Deux règles portent tout ce fichier.
//!
//! **Le retour est une recopie.** [`Ancre`] est littéralement les quatre champs
//! de [`Camera`], rangés tels quels. Revenir ne reconstruit rien : ni
//! repliement, ni normalisation, ni « repose la caméra au-dessus du sol ».
//! Toute reconstruction serait une occasion de dériver, et le banc n'aurait
//! plus rien à mesurer — sinon sa propre indulgence.
//!
//! **Le portail est un point du monde, pas une case de la grille.** Sa place
//! est calculée une fois par `cube::direction`, et c'est cette place-là qui
//! décide de la traversée. Une distance en coordonnées ne prédit pas une
//! distance parcourue (D27) : près d'un coin les deux divergent franchement, et
//! `--diag` dit de combien.

use crate::cube::{RAYON, direction};
use crate::monde::Generateur;
use crate::poche::{self, CameraPlate, Poche};
use crate::vue3d::Camera;
use glam::{DVec3, Vec2, Vec3};

/// Combien de temps la fenêtre tient avant de se refermer d'elle-même.
///
/// D16 dit une à deux heures ; le banc dit quatre-vingt-dix secondes. Ce qui
/// s'éprouve ici n'est pas la durée, c'est le fait que la sortie ne soit pas un
/// choix du joueur — D8 : on ne sort pas d'un donjon, on en est expulsé.
pub const DUREE_FENETRE: f32 = 90.0;

/// La nappe : sa largeur et sa hauteur, en blocs.
///
/// Le maillage et le test de franchissement les lisent tous les deux ici. Tant
/// que la traversée était une téléportation déclenchée par une proximité, un
/// écart entre les deux ne se voyait pas. Depuis qu'on passe *à travers*, la
/// nappe qu'on voit et l'ouverture qu'on franchit doivent être la même chose.
pub const LARGEUR_NAPPE: f32 = 3.0;
pub const HAUTEUR_NAPPE: f32 = 3.0;

/// Le centre de la nappe au-dessus du pied du cadre, en blocs.
pub const CENTRE_NAPPE: f32 = 2.5;

/// De combien de blocs devant la caméra le portail se pose.
const PORTEE_POSE: f32 = 8.0;

// --------------------------------------------------------------------------
// L'ancre
// --------------------------------------------------------------------------

/// Les quatre champs de [`Camera`], tels quels.
///
/// C'est ce qui rend le retour exact au bit près : il n'y a rien à recalculer.
/// `regard` étant un vecteur du **monde** (et non un cap rangé dans la grille),
/// il reste valable sans retouche — c'est le bénéfice direct du second volet de
/// D27, et le seul endroit du prototype où il se paie en confort.
#[derive(Clone, Copy)]
pub struct Ancre {
    pub face: u8,
    pub position: Vec3,
    pub regard: Vec3,
    pub tangage: f32,
}

impl Ancre {
    pub fn poser(cam: &Camera) -> Self {
        Self {
            face: cam.face,
            position: cam.position,
            regard: cam.regard,
            tangage: cam.tangage,
        }
    }

    /// Recopie. Rien d'autre, jamais.
    pub fn restituer(&self, cam: &mut Camera) {
        cam.face = self.face;
        cam.position = self.position;
        cam.regard = self.regard;
        cam.tangage = self.tangage;
    }

    /// Écart avec une caméra, en blocs et en degrés — la mesure de `--diag`.
    pub fn ecart(&self, cam: &Camera) -> (f32, f32) {
        let d = (cam.position - self.position).length();
        let c = self
            .regard
            .normalize_or_zero()
            .dot(cam.regard.normalize_or_zero())
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees();
        (d, c)
    }

    /// Les mêmes quatre champs, au bit près.
    pub fn identique_au_bit(&self, cam: &Camera) -> bool {
        let bits = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        self.face == cam.face
            && bits(self.position) == bits(cam.position)
            && bits(self.regard) == bits(cam.regard)
            && self.tangage.to_bits() == cam.tangage.to_bits()
    }
}

// --------------------------------------------------------------------------
// Le portail
// --------------------------------------------------------------------------

pub struct Portail {
    /// Sa case, canonique.
    pub face: u8,
    pub u: i32,
    pub v: i32,
    pub z: i32,
    /// Sa place dans le monde, calculée une fois. C'est elle qui décide de la
    /// traversée — jamais `(u, v)`.
    pub lieu: DVec3,
    /// Les deux axes de son cadre, en coordonnées de sa face. Ils viennent du
    /// regard du joueur décomposé **au lieu du portail**, si bien que le cadre
    /// est carré dans le monde et non dans la grille. Près d'un coin les deux
    /// ne se ressemblent pas : les tangentes s'y coupent à 120°.
    pub axe_droite: Vec2,
    pub axe_avant: Vec2,
    /// Son repère **dans le monde**, orthonormé : droite, avant, haut.
    ///
    /// Il existait déjà au moment de la pose — `droite()`, `avant_plat()` et la
    /// verticale locale — et n'était jusqu'ici que décomposé en axes de grille
    /// pour le maillage du cadre. Le garder tel quel est ce qui rend
    /// [`Portail::vers_la_poche`] aussi court : de ce côté un repère quelconque
    /// posé sur une sphère, de l'autre les trois axes.
    pub droite3: Vec3,
    pub avant3: Vec3,
    pub haut3: Vec3,
    /// L'instance qui s'ouvrira derrière.
    pub graine: u32,
    /// Où le joueur revient quand la fenêtre se referme.
    ///
    /// Elle voyage avec le portail parce que D13 n'en fait qu'une seule chose :
    /// l'ancre *est* la porte et le point de retour. Les séparer inviterait à
    /// refermer l'un sans l'autre.
    pub retour: Ancre,
}

impl Portail {
    /// Pose un portail devant la caméra.
    ///
    /// Le placement passe par un **clone de la caméra** qu'on fait `avancer` :
    /// c'est le seul chemin béni du déplacement, il découpe le pas à chaque
    /// bord franchi, et il transporte le regard correctement. Le portail peut
    /// donc légitimement atterrir de l'autre côté d'une arête, et son cadre y
    /// reste droit.
    ///
    /// `viser` aurait rendu un bloc, mais aucun repère orienté à cet endroit —
    /// et reconstruire l'orientation depuis la caméra du joueur, sur une autre
    /// face, est exactement l'erreur que D27 décrit.
    pub fn ouvrir(gen: &Generateur, cam: &Camera, graine: u32) -> Self {
        let retour = Ancre::poser(cam);
        let mut pose = *cam;
        let d3 = pose.avant_plat() * PORTEE_POSE;
        let (du, dv) = pose.vers_coordonnees(d3);
        pose.avancer(Vec3::new(du, dv, 0.0));

        let (u, v) = (pose.position.x.floor() as i32, pose.position.y.floor() as i32);

        // À hauteur d'œil, pas au sol.
        //
        // La caméra du banc vole à trente blocs au-dessus du terrain : un
        // portail planté au sol serait ouvert sous les pieds du joueur, et il
        // faudrait descendre pour le franchir. On le pose donc devant le
        // regard — c'est ce qui a été demandé, et c'est ce qui se franchit en
        // marchant droit. Il ne s'enterre pas pour autant : le sol le repousse.
        let sol = gen.hauteur(pose.face, u, v).max(crate::monde::NIVEAU_MER as f32) as i32;
        let z = (pose.position.z.floor() as i32 - 2).max(sol + 1);

        // Les deux axes du cadre, pris sur place. `droite × avant` vaut la
        // verticale locale (droite = regard × haut, avant = regard), donc le
        // trièdre (droite, avant, Z) est direct — le maillage peut s'en servir
        // tel quel sans se retourner.
        //
        // **Et surtout : on ne les normalise pas.** `vers_coordonnees` reçoit un
        // vecteur du monde de longueur 1 et rend le déplacement de grille qui
        // vaut un bloc *pour de vrai*. Le normaliser rendrait le cadre long
        // d'une unité de **coordonnée**, ce qui n'est pas une longueur : sur la
        // sphère, un pas de grille vaut entre 0,69 et 1,00 bloc selon l'endroit
        // de la face (D27). Le portail du présent rétrécissait donc près des
        // coins, quand celui du passé — où le monde est le plan — gardait
        // toujours ses trois blocs. Les deux nappes n'avaient pas la même
        // taille, et l'ouverture qu'on franchit ne coïncidait pas avec celle
        // qu'on voit : `franchi` mesure son rectangle en blocs du monde, lui.
        //
        // L'échelle est prise au centre du cadre et vaut pour ses trois blocs.
        // C'est une approximation au premier ordre, et elle est bonne : la
        // taille d'un bloc ne varie pas assez vite pour se voir sur si peu.
        let axe = |d3: Vec3| {
            let (a, b) = pose.vers_coordonnees(d3);
            Vec2::new(a, b)
        };
        let (droite3, avant3) = (pose.droite(), pose.avant_plat());
        let haut3 = Vec3::from_array(
            direction(pose.face, pose.position.x as f64, pose.position.y as f64)
                .map(|x| x as f32),
        )
        .normalize();
        // Puis on ajuste contre le rendu lui-même. Les tangentes de `base()`
        // sont prises au niveau de la mer, or le cadre se dessine à l'altitude
        // du portail, où le même pas de grille est plus long — d'un rayon sur
        // l'autre, quatre pour cent ici. Plutôt que de corriger à la main un
        // facteur qu'on aurait à tenir à jour, on mesure la largeur rendue par
        // `direction`, la fonction dont le shader se sert, et on la ramène à
        // sa valeur. Deux passes suffisent : la correction converge d'un coup.
        let rayon = RAYON + z as f64 + CENTRE_NAPPE as f64;
        let (cu, cv) = (u as f64 + 0.5, v as f64 + 0.5);
        let mut axe_droite = axe(droite3);
        let mut axe_avant = axe(avant3);
        for _ in 0..2 {
            axe_droite = ajuster(pose.face, cu, cv, rayon, axe_droite, LARGEUR_NAPPE);
            axe_avant = ajuster(pose.face, cu, cv, rayon, axe_avant, LARGEUR_NAPPE);
        }

        Self {
            face: pose.face,
            u,
            v,
            z,
            lieu: place(pose.face, u, v, z),
            axe_droite,
            axe_avant,
            droite3,
            avant3,
            haut3,
            graine,
            retour,
        }
    }

    /// Le pas passe-t-il **à travers l'ouverture** ? Si oui, à quelle fraction ?
    ///
    /// Le test porte sur le plan de la nappe, et sur le rectangle qu'elle
    /// occupe — pas sur une sphère autour du portail. Tant que la traversée
    /// était une téléportation, une sphère suffisait : elle disait « assez
    /// près », et le reste était un saut. Maintenant qu'on passe physiquement,
    /// c'est l'ouverture qui décide, et elle a des bords : frôler le montant ne
    /// fait pas entrer.
    ///
    /// Et c'est bien le **segment** qui est testé, pas le point d'arrivée : le
    /// curseur de vitesse va jusqu'à 400 blocs par seconde, soit près de sept
    /// blocs par image. Un test ponctuel traverserait sans voir.
    ///
    /// Rend la fraction du pas à laquelle le plan est coupé, pour qu'on puisse
    /// s'y arrêter net et repartir de l'autre côté.
    pub fn franchi(&self, avant: DVec3, apres: DVec3) -> Option<f32> {
        let normale = DVec3::new(
            self.avant3.x as f64,
            self.avant3.y as f64,
            self.avant3.z as f64,
        );
        let d0 = (avant - self.lieu).dot(normale);
        let d1 = (apres - self.lieu).dot(normale);

        // Il faut changer de côté. Le sens n'importe pas : on entre par devant,
        // on ressort par derrière, et c'est la même ouverture.
        if d0 == d1 || (d0 > 0.0) == (d1 > 0.0) {
            return None;
        }
        let t = d0 / (d0 - d1);
        if !(0.0..=1.0).contains(&t) {
            return None;
        }

        // Le point de passage tombe-t-il dans le rectangle de la nappe ?
        let p = avant + (apres - avant) * t;
        let ecart = p - self.lieu;
        let long = |v: Vec3| {
            ecart.dot(DVec3::new(v.x as f64, v.y as f64, v.z as f64)).abs()
        };
        if long(self.droite3) > (LARGEUR_NAPPE * 0.5) as f64
            || long(self.haut3) > (HAUTEUR_NAPPE * 0.5) as f64
        {
            return None;
        }
        Some(t as f32)
    }

    /// Le plan de coupe pour l'aperçu du **présent**, vu depuis la poche.
    ///
    /// La caméra virtuelle se tient de l'autre côté de la nappe et regarde vers
    /// le joueur : c'est donc le relief situé derrière elle qu'il faut retirer.
    pub fn coupe_entree(&self) -> [f32; 4] {
        let n = -self.avant3;
        let l = Vec3::new(self.lieu.x as f32, self.lieu.y as f32, self.lieu.z as f32);
        [n.x, n.y, n.z, -n.dot(l)]
    }

    /// La caméra du passé, vue à travers la nappe.
    ///
    /// La transformation est une rotation, et rien de plus : on exprime la
    /// caméra réelle dans le repère du portail d'entrée, puis on relit les trois
    /// nombres obtenus dans celui du portail de sortie. Comme ce dernier est
    /// aligné sur les axes de la poche, « relire » est littéralement l'identité.
    ///
    /// **Ce que ça doit à D27.** `Camera::regard` est un vecteur du monde : son
    /// orientation se transporte par un produit scalaire, sans que personne ait
    /// à savoir sur quelle face on l'écrivait. Un cap rangé dans la grille aurait
    /// demandé d'être réinterprété d'un côté à l'autre — c'est-à-dire
    /// reconstruit, avec la correction d'un quart de tour qui n'est juste que si
    /// les tangentes sont perpendiculaires.
    ///
    /// **Et ce que la courbure ne coûte pas.** Entre un monde courbe et un monde
    /// plat, une transformation rigide n'est exacte que localement. Sur les trois
    /// blocs de la nappe, la flèche vaut `w²/8r` ≈ 0,0006 bloc pour un rayon de
    /// 1 888 : plusieurs ordres de grandeur sous le pixel. Localement, la sphère
    /// est plate, et c'est tout ce qu'on lui demande ici.
    pub fn vers_la_poche(&self, cam: &Camera) -> (Vec3, Vec3, Vec3) {
        let (position, avant, haut) = cam.repere_3d(RAYON);
        let ici = Vec3::new(position.x as f32, position.y as f32, position.z as f32);
        (
            poche::SORTIE + self.dans_le_repere(ici - self.centre()),
            self.dans_le_repere(avant).normalize(),
            self.dans_le_repere(haut).normalize(),
        )
    }

    /// Le centre de la nappe, en `f32`.
    pub fn centre(&self) -> Vec3 {
        Vec3::new(self.lieu.x as f32, self.lieu.y as f32, self.lieu.z as f32)
    }

    /// Un vecteur du monde, exprimé dans le repère du portail.
    pub fn dans_le_repere(&self, v: Vec3) -> Vec3 {
        Vec3::new(v.dot(self.droite3), v.dot(self.avant3), v.dot(self.haut3))
    }

    /// L'inverse : trois nombres du repère du portail, rendus au monde.
    pub fn depuis_le_repere(&self, v: Vec3) -> Vec3 {
        self.droite3 * v.x + self.avant3 * v.y + self.haut3 * v.z
    }

    /// La caméra du joueur, transportée dans la poche.
    ///
    /// C'est la même transformation que celle de l'aperçu, rendue sous la forme
    /// qu'attend le joueur. Que les deux soient la même chose est exactement ce
    /// qui rend la traversée continue : ce que la nappe montrait à l'image
    /// précédente est ce qu'on obtient à l'image suivante.
    pub fn camera_de_la_poche(&self, cam: &Camera) -> CameraPlate {
        let (position, avant, _) = self.vers_la_poche(cam);
        CameraPlate {
            position,
            regard: Vec3::new(avant.x, avant.y, 0.0).normalize_or(Vec3::Y),
            tangage: avant.z.clamp(-1.0, 1.0).asin(),
        }
    }

    /// Et le retour : la caméra de la poche, transportée sur la sphère.
    ///
    /// C'est ici que le prototype encaisse la facture de D27 — et qu'il montre
    /// qu'elle est payée. Rendre un point 3D à la grille demande
    /// `depuis_direction`, l'inverse de la projection. Le banc mesure cet
    /// aller-retour au **millième de bloc** ; sans cette garantie, sortir du
    /// passé rendrait le joueur à côté de là où il est.
    pub fn camera_de_la_sphere(&self, plate: &CameraPlate) -> Camera {
        let monde = self.centre() + self.depuis_le_repere(plate.position - poche::SORTIE);
        let rayon = monde.length();
        let (face, u, v) = crate::cube::depuis_direction(
            [monde.x as f64 / rayon as f64, monde.y as f64 / rayon as f64, monde.z as f64 / rayon as f64],
        );

        let (_, avant, _) = plate.repere();
        let avant3 = self.depuis_le_repere(avant).normalize();
        let haut3 = monde / rayon;

        let mut cam = Camera {
            face,
            position: Vec3::new(u as f32, v as f32, rayon - RAYON as f32),
            // Amorçage : remplacé juste après, une fois la verticale connue.
            regard: Vec3::X,
            tangage: avant3.dot(haut3).clamp(-1.0, 1.0).asin(),
        };
        let plat = avant3 - haut3 * avant3.dot(haut3);
        cam.regard = plat.normalize_or(cam.avant_plat());
        let _ = cam.replier();
        cam
    }

    /// La distance qui reste à parcourir, dans le monde.
    pub fn distance(&self, cam: &Camera) -> f64 {
        (cam.repere_3d(RAYON).0 - self.lieu).length()
    }

    /// La distance qu'aurait donnée un test en coordonnées de face.
    ///
    /// Elle n'existe que pour être affichée à côté de la vraie par `--diag` :
    /// c'est la mesure de ce qu'on aurait eu en rangeant l'ancre dans la
    /// grille. Rien du programme ne s'en sert pour décider quoi que ce soit.
    pub fn distance_en_coordonnees(&self, cam: &Camera) -> f32 {
        if cam.face != self.face {
            // Deux faces différentes : la comparaison n'a même pas de sens, ce
            // qui est déjà la réponse.
            return f32::INFINITY;
        }
        let d = Vec3::new(
            cam.position.x - (self.u as f32 + 0.5),
            cam.position.y - (self.v as f32 + 0.5),
            cam.position.z - (self.z as f32 + 2.0),
        );
        d.length()
    }
}

/// Ramène un axe de grille à la longueur voulue, **mesurée comme le rendu la
/// mesure**.
///
/// C'est le seul chiffre opposable : une longueur en coordonnées ne dit rien de
/// ce qu'on voit, puisqu'un pas de grille vaut entre 0,69 et 1,00 bloc selon
/// l'endroit de la face — et davantage encore avec l'altitude, la case étant un
/// prisme radial qui s'élargit en montant.
fn ajuster(face: u8, u: f64, v: f64, rayon: f64, axe: Vec2, longueur: f32) -> Vec2 {
    let place = |s: f64| {
        DVec3::from_array(direction(face, u + axe.x as f64 * s, v + axe.y as f64 * s)) * rayon
    };
    let demi = longueur as f64 * 0.5;
    let mesure = (place(demi) - place(-demi)).length();
    if mesure < 1e-9 {
        return axe;
    }
    axe * (longueur as f64 / mesure) as f32
}

/// La place du **centre de la nappe** dans le monde.
fn place(face: u8, u: i32, v: i32, z: i32) -> DVec3 {
    DVec3::from_array(direction(face, u as f64 + 0.5, v as f64 + 0.5))
        * (RAYON + z as f64 + CENTRE_NAPPE as f64)
}

// --------------------------------------------------------------------------
// Le séjour
// --------------------------------------------------------------------------

/// Où se trouve le joueur. C'est l'aiguillage, et le seul endroit du programme
/// qui sache que deux mondes existent.
pub enum Sejour {
    Sphere,
    Poche { retour: Ancre, poche: Poche, cam: CameraPlate },
}

// Le minuteur de la fenêtre a quitté cet enum le jour où l'on a pu **revenir
// par le portail** : la fenêtre continue de s'user pendant qu'on est reparti
// dans le présent, et se refermera là où le joueur se trouvera. Elle n'est donc
// plus une propriété du séjour, mais de l'ancre — voir `App::fenetre`.

impl Sejour {
    pub fn dans_la_poche(&self) -> bool {
        matches!(self, Sejour::Poche { .. })
    }
}
