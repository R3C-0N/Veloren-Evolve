//! Le globe : la carte du monde relue sur la planète (D38).
//!
//! Le patron de cube est un rangement, pas une image du monde. L'afficher tel
//! quel montre six faces en croix et dix emplacements morts, et place deux
//! lieux voisins du cube aux deux bouts de l'écran. On le lit donc **par la
//! géométrie** : chaque pixel de l'écran remonte à une direction, la direction
//! à une face, et la face à un pixel du patron.
//!
//! Trois choses vivent ici :
//!
//! - [`Vue`] — le repère orthonormé sous lequel on regarde la planète, et les
//!   deux sens de passage entre une direction du monde et un point de l'écran ;
//! - [`Inverse`] — l'inverse de la projection conforme, tabulé une fois, parce
//!   qu'un Newton par pixel n'est pas payable ;
//! - [`Globe`] — le tampon rastérisé, refait **uniquement** quand la vue
//!   change, et échangé dans l'interface par `Ui::replace_graphic`.
//!
//! **Aucune étape n'a de pôle, et c'est délibéré.** Une première version
//! passait par une nappe équirectangulaire ; ses colonnes s'effondrent aux
//! pôles, des dizaines d'entre elles y retombaient sur le même pixel du patron,
//! et il en sortait une rosace de secteurs dès qu'on zoomait. Un cube n'a pas
//! de pôle : ses six faces sont équivalentes.
//!
//! La règle du sens unique de D27 vaut ici comme ailleurs : un clic est
//! redressé une fois, par [`Vue::depuis_ecran`], et le monde n'est ensuite
//! interrogé qu'à plat, par `cube::depuis_direction`.

use common::{
    terrain::{MapSizeLg, TerrainChunkSize, conforme, cube},
    vol::RectVolSize,
};
use image::RgbaImage;
use std::{f64::consts::TAU, sync::Arc};
use vek::*;

use crate::ui::{Graphic, Ui, img_ids};
use rayon::prelude::*;

/// Côté du tampon de la grande carte, en pixels — exactement le côté du cadre
/// dans `hud::map`, si bien qu'un pixel du tampon est un point de l'écran et
/// que rien n'a besoin d'être remis à l'échelle.
///
/// Il est **constant** : `replace_graphic` ne réemploie la place d'atlas que si
/// les dimensions ne bougent pas (`ui/graphic/mod.rs`). C'est aussi lui qui
/// fixe le coût d'un glisser — le seul bouton à tourner si la rotation
/// accroche.
pub const COTE: u32 = 760;

/// Côté du tampon de la minicarte.
pub const COTE_MINI: u32 = 256;

// --------------------------------------------------------------------------
// Le repère de vue
// --------------------------------------------------------------------------

/// Le repère orthonormé sous lequel la planète est regardée : ce qui va vers la
/// droite de l'écran, ce qui va vers le haut, et ce qui vient vers l'œil.
///
/// C'est un repère du **monde**, jamais un cap rangé dans la grille — la leçon
/// de D27 : un angle posé dans les coordonnées du patron hérite de son
/// cisaillement, et près d'un coin les deux tangentes se coupent à 120°.
#[derive(Clone, Copy, Debug)]
pub struct Vue {
    pub droite: Vec3<f64>,
    pub haut: Vec3<f64>,
    pub avant: Vec3<f64>,
}

impl Vue {
    /// Le repère centré sur une longitude et une latitude, nord vers le haut.
    pub fn depuis_angles(lon: f64, lat: f64) -> Self {
        let (sin_lon, cos_lon) = lon.sin_cos();
        let (sin_lat, cos_lat) = lat.sin_cos();
        Self {
            droite: Vec3::new(-sin_lon, cos_lon, 0.0),
            haut: Vec3::new(-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat),
            avant: Vec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat),
        }
    }

    /// Le repère local en un lieu, tourné d'un cap.
    ///
    /// Les deux tangentes de `cube::repere` **ne sont pas orthogonales** — près
    /// d'un coin elles se coupent à 120°, et c'est le sujet, pas un défaut.
    /// Dessiner demande pourtant un repère droit : on redresse donc la seconde
    /// contre la première, et on assume que la minicarte cisaille très
    /// légèrement le terrain aux abords des huit coins. C'est une image, pas
    /// une mesure — rien de ce qui décide quoi que ce soit ne passe par là.
    pub fn depuis_lieu(map: MapSizeLg, lieu: cube::Lieu, cap: f64) -> Self {
        let (avant, tu, _tv) = cube::repere(map, lieu);
        let avant = avant.normalized();
        let brut = tu - avant * tu.dot(avant);
        // Au cas très improbable où `tu` serait colinéaire à la verticale, on
        // reprend le nord géographique plutôt que de rendre un repère dégénéré.
        let droite = if brut.magnitude_squared() > 1e-12 {
            brut.normalized()
        } else {
            return Self::depuis_angles(avant.y.atan2(avant.x), avant.z.clamp(-1.0, 1.0).asin());
        };
        let haut = avant.cross(droite);
        let (s, c) = cap.sin_cos();
        Self {
            droite: droite * c - haut * s,
            haut: droite * s + haut * c,
            avant,
        }
    }

    /// Une direction du monde, décomposée sur le repère : `(x, y)` sont les
    /// coordonnées du disque, `z` est positif du côté visible.
    #[inline]
    pub fn decomposer(&self, d: Vec3<f64>) -> Vec3<f64> {
        Vec3::new(d.dot(self.droite), d.dot(self.haut), d.dot(self.avant))
    }

    /// Le point du disque unité vu à l'écran, ou `None` si le point est sur la
    /// face cachée.
    ///
    /// C'est ici, et nulle part ailleurs, que la moitié cachée du globe est
    /// écartée. Sans ce test, un site des antipodes viendrait se poser sur la
    /// face visible, à l'endroit exact de son symétrique.
    #[inline]
    pub fn projeter(&self, d: Vec3<f64>) -> Option<Vec2<f64>> {
        let v = self.decomposer(d);
        (v.z > 0.0).then(|| Vec2::new(v.x, v.y))
    }

    /// L'inverse : de quelle direction du monde vient un point du disque unité.
    ///
    /// Rend `None` hors du disque — là, l'écran ne montre pas la planète, et il
    /// n'y a rien à désigner.
    #[inline]
    pub fn depuis_ecran(&self, p: Vec2<f64>) -> Option<Vec3<f64>> {
        let q = p.magnitude_squared();
        (q <= 1.0).then(|| self.droite * p.x + self.haut * p.y + self.avant * (1.0 - q).sqrt())
    }
}

// --------------------------------------------------------------------------
// La planète, telle que l'interface a besoin de la connaître
// --------------------------------------------------------------------------

/// Tout ce que les deux cartes ont besoin de savoir de la planète.
///
/// Rendue `None` quand le monde est plat : c'est ce `None`, et pas un drapeau
/// glissé dans les widgets, qui laisse le chemin d'origine intact.
#[derive(Clone, Copy, Debug)]
pub struct Planete {
    pub map: MapSizeLg,
    /// Le rayon, en blocs.
    pub rayon: f64,
    /// L'arête d'une face, en chunks — donc en pixels du patron.
    pub face_chunks: u32,
}

impl Planete {
    /// La planète d'un client, ou `None` si son monde est un plan.
    pub fn depuis(client: &client::Client) -> Option<Self> {
        let map = client.state().terrain().map_size_lg();
        map.est_cubique().then(|| Self {
            map,
            rayon: cube::rayon(map),
            face_chunks: cube::face_chunks(map).max(1) as u32,
        })
    }

    /// La direction du monde d'une position du patron.
    ///
    /// `None` sur un des dix emplacements morts — il n'y a là aucun lieu, et
    /// donc rien à placer sur le globe.
    #[inline]
    pub fn direction(&self, wpos: Vec2<f64>) -> Option<Vec3<f64>> {
        cube::direction(self.map, wpos)
    }

    /// La position du patron d'une direction du monde.
    #[inline]
    pub fn wpos_de(&self, d: Vec3<f64>) -> Vec2<f64> {
        cube::depuis_direction(self.map, d).wpos(self.map)
    }

    /// Le rayon du globe à l'écran, en pixels, pour un zoom en pixels par
    /// chunk. L'unité du réglage `interface.map_zoom` ne change donc pas.
    pub fn rayon_px(&self, zoom: f64) -> f64 {
        self.rayon * zoom / TerrainChunkSize::RECT_SIZE.x as f64
    }

    /// Le zoom auquel la planète entière tient tout juste dans un cadre.
    /// C'est le cran le plus large : au-delà on ne ferait que l'éloigner.
    pub fn zoom_ajuste(&self, cote: f64) -> f64 {
        cote * TerrainChunkSize::RECT_SIZE.x as f64 / (2.0 * self.rayon)
    }

    /// La longitude et la latitude d'une position du patron.
    fn angles(&self, wpos: Vec2<f64>) -> (f64, f64) {
        let d = self.direction(wpos).unwrap_or(Vec3::unit_z());
        (d.y.atan2(d.x), d.z.clamp(-1.0, 1.0).asin())
    }

    /// Borne un glissement : la longitude boucle, la latitude s'arrête aux
    /// pôles.
    ///
    /// La borne se prend **relativement au joueur**, et c'est le point : bornée
    /// à ±90° dans l'absolu, elle empêcherait un joueur de l'hémisphère nord
    /// d'atteindre le pôle sud. Bornée trop large, elle laisserait un jeu mort
    /// dans le glisser — on tirerait sans que rien ne bouge, puis il faudrait
    /// tirer d'autant en sens inverse avant que le globe ne reparte.
    pub fn borner_glissement(&self, joueur: Vec2<f64>, drag: Vec2<f64>) -> Vec2<f64> {
        use std::f64::consts::{FRAC_PI_2, PI};
        // Un cheveu avant le pôle : pile dessus, la longitude n'a plus de sens
        // et le repère de vue devient dégénéré.
        let bord = FRAC_PI_2 - 1.0e-4;
        let (_, lat) = self.angles(joueur);
        Vec2::new(
            (drag.x + PI).rem_euclid(TAU) - PI,
            drag.y.clamp(lat - bord, lat + bord),
        )
    }

    /// Le repère de la grande carte : centré sur le lieu du joueur, décalé du
    /// glissement, nord vers le haut.
    pub fn vue_carte(&self, joueur: Vec2<f64>, drag: Vec2<f64>) -> Vue {
        let (lon, lat) = self.angles(joueur);
        Vue::depuis_angles(lon - drag.x, lat - drag.y)
    }

    /// Le repère de la minicarte : posé sur le lieu du joueur, tourné de son
    /// cap.
    pub fn vue_locale(&self, joueur: Vec2<f64>, cap: f64) -> Option<Vue> {
        cube::lieu_de(self.map, joueur).map(|l| Vue::depuis_lieu(self.map, l, cap))
    }
}

/// Le rayon du globe de la minicarte, en pixels de son tampon.
///
/// La minicarte ne se règle pas en pixels par chunk mais en « combien de la
/// carte on voit » : sa fenêtre fait `max_zoom / zoom` chunks de côté. On
/// traduit cette largeur en angle sur la planète, puis en rayon de disque —
/// c'est la même orthographie que la grande carte, prise de très près.
pub fn rayon_mini(planete: &Planete, zoom: f64, taille: Vec2<u16>) -> f64 {
    let max_zoom = taille.reduce_partial_max() as f64;
    let fenetre = (max_zoom / zoom.max(1.0)).max(1.0);
    COTE_MINI as f64 * planete.rayon / (fenetre * TerrainChunkSize::RECT_SIZE.x as f64)
}

// --------------------------------------------------------------------------
// L'inverse de la projection, tabulé
// --------------------------------------------------------------------------

/// Côté de la table inverse. Même finesse que la table conforme : rien ne
/// justifierait d'en avoir moins, et la fonction y est lisse.
const COTE_INVERSE: usize = 513;

/// L'inverse de la projection conforme, tabulé sur le carré gnomonique.
///
/// `conforme::Table::depuis_locale` fait un Newton de douze itérations : c'est
/// le bon prix pour un clic, jamais pour un demi-million de pixels à chaque
/// image. On le paie donc une fois, sur une grille régulière de `(a, b)`.
///
/// Le domaine est exactement `[-1, 1]²` : dans ces coordonnées le bord d'une
/// face vaut `a = ±1` (`conforme.rs`), si bien qu'il n'y a ni coin perdu ni
/// bord à deviner.
///
/// **C'est ce qui remplace la nappe équirectangulaire, et pour une raison de
/// forme, pas de vitesse.** Une nappe a des pôles — ses colonnes s'y
/// effondrent, des dizaines d'entre elles retombent sur le même pixel du
/// patron, et le plus proche voisin en fait une rosace de secteurs, bien
/// visible dès qu'on zoome. Un cube n'a pas de pôle : les six faces sont
/// équivalentes, et le défaut n'a nulle part où naître.
struct Inverse {
    st: Vec<[f32; 2]>,
}

static INVERSE: std::sync::OnceLock<Inverse> = std::sync::OnceLock::new();

fn inverse() -> &'static Inverse { INVERSE.get_or_init(Inverse::construire) }

impl Inverse {
    fn construire() -> Self {
        let n = COTE_INVERSE;
        let table = conforme::table();
        let mut st = vec![[0.0f32; 2]; n * n];
        st.par_chunks_mut(n).enumerate().for_each(|(j, ligne)| {
            let b = 2.0 * j as f64 / (n - 1) as f64 - 1.0;
            for (i, sortie) in ligne.iter_mut().enumerate() {
                let a = 2.0 * i as f64 / (n - 1) as f64 - 1.0;
                // La direction locale dont `(a, b)` est la gnomonique : le
                // troisième terme vaut 1 par définition du plan tangent.
                let l = (1.0 + a * a + b * b).sqrt();
                let (s, t) = table.depuis_locale([a / l, b / l, 1.0 / l]);
                *sortie = [s as f32, t as f32];
            }
        });
        Self { st }
    }

    /// Bilinéaire sur la grille. La fonction est lisse — pas de couture ici,
    /// contrairement au patron : on peut filtrer sans rien mélanger.
    #[inline]
    fn lire(&self, a: f64, b: f64) -> (f64, f64) {
        let n = COTE_INVERSE;
        let m = (n - 1) as f64;
        let x = ((a + 1.0) * 0.5 * m).clamp(0.0, m);
        let y = ((b + 1.0) * 0.5 * m).clamp(0.0, m);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as usize, y0 as usize);
        let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
        let p = |i: usize, j: usize| self.st[j * n + i];
        let (c00, c10, c01, c11) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        let mel = |a: f32, b: f32, f: f64| a as f64 * (1.0 - f) + b as f64 * f;
        (
            mel(
                mel(c00[0], c10[0], fx) as f32,
                mel(c01[0], c11[0], fx) as f32,
                fy,
            ),
            mel(
                mel(c00[1], c10[1], fx) as f32,
                mel(c01[1], c11[1], fx) as f32,
                fy,
            ),
        )
    }
}

// --------------------------------------------------------------------------
// L'échantillonnage du patron
// --------------------------------------------------------------------------

/// La couleur du patron dans une direction du monde.
///
/// Trois pas, et aucun ne connaît de pôle : la face est celle dont la normale
/// domine, `(a, b)` est la gnomonique dans son repère, et la table inverse rend
/// la place dans la face.
///
/// Le filtrage est bilinéaire **à l'intérieur de la face**, et les coordonnées
/// y sont bornées : deux faces voisines dans le patron ne le sont pas forcément
/// dans le monde, et laisser le filtre déborder ferait baver l'une sur l'autre
/// exactement aux coutures. Au pire on répète le pixel du bord sur un demi-
/// pixel, ce qui ne se voit pas ; mélanger deux faces, si.
#[inline]
fn echantillon(patron: &RgbaImage, planete: &Planete, d: Vec3<f64>) -> [u8; 4] {
    // La face dont la normale domine. `local.z` est ce maximum, donc au moins
    // `1/√3` : la division gnomonique ne peut pas exploser.
    let mut face = 0usize;
    let mut meilleur = f64::MIN;
    for f in 0..6 {
        let n = cube::BASES[f].n;
        let dot = d.x * n[0] as f64 + d.y * n[1] as f64 + d.z * n[2] as f64;
        if dot > meilleur {
            meilleur = dot;
            face = f;
        }
    }

    let base = cube::BASES[face];
    let proj = |v: [i32; 3]| d.x * v[0] as f64 + d.y * v[1] as f64 + d.z * v[2] as f64;
    let z = meilleur;
    let (s, t) = inverse().lire(proj(base.r) / z, proj(base.h) / z);

    let fc = planete.face_chunks as f64;
    let (col, ligne) = cube::PATRON[face];
    // Centres de texels : `s = -1` désigne le bord de la face, donc le centre
    // du premier chunk est un demi-texel plus loin.
    let x = ((s + 1.0) * 0.5 * fc - 0.5).clamp(0.0, fc - 1.0);
    let y = ((t + 1.0) * 0.5 * fc - 0.5).clamp(0.0, fc - 1.0);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let borne = |k: f64| k.clamp(0.0, fc - 1.0) as u32;
    let (dx, dy) = (
        col as u32 * planete.face_chunks,
        ligne as u32 * planete.face_chunks,
    );
    let p = |i: f64, j: f64| patron.get_pixel(dx + borne(i), dy + borne(j)).0;
    let (c00, c10, c01, c11) = (
        p(x0, y0),
        p(x0 + 1.0, y0),
        p(x0, y0 + 1.0),
        p(x0 + 1.0, y0 + 1.0),
    );

    let mut out = [0u8; 4];
    for k in 0..4 {
        let haut = c00[k] as f64 * (1.0 - fx) + c10[k] as f64 * fx;
        let bas = c01[k] as f64 * (1.0 - fx) + c11[k] as f64 * fx;
        out[k] = (haut * (1.0 - fy) + bas * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

// --------------------------------------------------------------------------
// Le rasteur
// --------------------------------------------------------------------------

/// Ce qui décide qu'un tampon est encore bon.
///
/// Sans cette porte, la carte immobile se rastériserait à chaque image pour
/// rien. C'est la même garde que celle de `VoxelMinimap`, qui ne recompose que
/// lorsque le joueur a changé de chunk.
#[derive(Clone, Copy, PartialEq)]
struct Etat {
    couche: usize,
    /// Le repère, arrondi : deux vues à un cent-millième de radian donnent le
    /// même pixel, et comparer des flottants exacts ferait rastériser sur du
    /// bruit numérique.
    repere: [i64; 9],
    /// Le rayon du globe en pixels, arrondi au centième.
    rayon: i64,
}

impl Etat {
    fn nouveau(couche: usize, vue: &Vue, rayon: f64) -> Self {
        let q = |x: f64| (x * 1.0e5).round() as i64;
        Self {
            couche,
            repere: [
                q(vue.droite.x),
                q(vue.droite.y),
                q(vue.droite.z),
                q(vue.haut.x),
                q(vue.haut.y),
                q(vue.haut.z),
                q(vue.avant.x),
                q(vue.avant.y),
                q(vue.avant.z),
            ],
            rayon: (rayon * 100.0).round() as i64,
        }
    }
}

/// Un tampon rastérisé et sa place dans l'interface.
pub struct Globe {
    cote: u32,
    tampon: RgbaImage,
    image: img_ids::Rotations,
    etat: Option<Etat>,
}

impl Globe {
    /// Réserve la place dans l'interface. Le tampon part transparent : la
    /// première image sera de toute façon rastérisée, l'état étant `None`.
    pub fn nouveau(ui: &mut Ui, cote: u32) -> Self {
        let tampon = RgbaImage::new(cote, cote);
        let image = ui.add_graphic_with_rotations(Graphic::Image(
            Arc::new(image::DynamicImage::ImageRgba8(tampon.clone())),
            None,
        ));
        Self {
            cote,
            tampon,
            image,
            etat: None,
        }
    }

    /// L'identifiant à passer à conrod. Un globe n'a pas de variante tournée :
    /// sa rotation est **dans le tampon**, pas dans le rectangle source.
    pub fn image(&self) -> conrod_core::image::Id { self.image.none }

    /// Rastérise si la vue a bougé, et rend `true` si l'image a changé.
    ///
    /// `rayon` est le rayon du globe **en pixels du tampon** : c'est lui, et
    /// non une découpe de source, qui porte le zoom. Au-delà de la moitié
    /// du côté, le globe déborde du cadre et l'on n'en voit qu'une calotte
    /// — ce qui est exactement ce qu'on attend d'un grossissement.
    pub fn maintain(
        &mut self,
        ui: &mut Ui,
        patron: &RgbaImage,
        planete: &Planete,
        couche: usize,
        vue: &Vue,
        rayon: f64,
    ) -> bool {
        let etat = Etat::nouveau(couche, vue, rayon);
        if self.etat == Some(etat) {
            return false;
        }
        self.etat = Some(etat);

        let cote = self.cote;
        let centre = cote as f64 / 2.0;
        // La couverture du dernier pixel : sans elle le limbe est en escalier,
        // et un globe cranté se remarque bien plus qu'un globe un peu flou.
        let lisse = 1.0 / rayon.max(1.0);

        self.tampon
            .as_mut()
            .par_chunks_mut(cote as usize * 4)
            .enumerate()
            .for_each(|(py, ligne)| {
                let y = -((py as f64 + 0.5) - centre) / rayon;
                for px in 0..cote as usize {
                    let x = ((px as f64 + 0.5) - centre) / rayon;
                    let r = (x * x + y * y).sqrt();
                    let pixel = &mut ligne[px * 4..px * 4 + 4];
                    if r > 1.0 + lisse {
                        pixel.copy_from_slice(&[0, 0, 0, 0]);
                        continue;
                    }
                    // Au ras du limbe, la normale file à l'horizontale et le
                    // moindre écart d'échantillonnage devient un pixel de
                    // couleur arbitraire : on prend le point du bord.
                    let p = if r > 1.0 {
                        Vec2::new(x, y) / r
                    } else {
                        Vec2::new(x, y)
                    };
                    let Some(d) = vue.depuis_ecran(p) else {
                        pixel.copy_from_slice(&[0, 0, 0, 0]);
                        continue;
                    };
                    let mut c = echantillon(patron, planete, d);
                    c[3] = (255.0 * ((1.0 + lisse - r) / lisse).clamp(0.0, 1.0)) as u8;
                    pixel.copy_from_slice(&c);
                }
            });

        ui.replace_graphic(
            self.image.none,
            Graphic::Image(
                Arc::new(image::DynamicImage::ImageRgba8(self.tampon.clone())),
                None,
            ),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carte() -> MapSizeLg { MapSizeLg::nouvelle_cubique(Vec2::new(6, 6)).unwrap() }

    /// **La table inverse dit la même chose que le Newton qu'elle remplace.**
    ///
    /// C'est le seul endroit où le raccourci pourrait mentir : `depuis_locale`
    /// reste la définition, la table n'en est qu'un cache. On balaie le carré
    /// gnomonique et on compare, en **chunks du patron** — l'unité dans
    /// laquelle l'erreur se verrait à l'écran.
    #[test]
    fn la_table_inverse_suit_le_newton() {
        let map = carte();
        let fc = cube::face_chunks(map) as f64;
        let table = conforme::table();
        let inv = inverse();
        let mut pire: f64 = 0.0;
        for i in 0..=200 {
            for j in 0..=200 {
                let a = 2.0 * i as f64 / 200.0 - 1.0;
                let b = 2.0 * j as f64 / 200.0 - 1.0;
                let l = (1.0 + a * a + b * b).sqrt();
                let (s0, t0) = table.depuis_locale([a / l, b / l, 1.0 / l]);
                let (s1, t1) = inv.lire(a, b);
                // De `[-1, 1]` vers des chunks : une demi-face par unité.
                pire = pire.max(((s1 - s0).abs()).max((t1 - t0).abs()) * fc / 2.0);
            }
        }
        assert!(pire < 0.05, "table inverse : {pire} chunk d'écart");
    }

    /// **Aucune direction ne tombe sur un emplacement mort.**
    ///
    /// Désormais c'est structurel — la face vient de la normale dominante, donc
    /// toujours l'une des six — mais le test reste : c'est l'invariant dont
    /// dépend toute la carte, et il coûte moins cher à vérifier qu'à supposer.
    /// Un carré noir sur la planète serait le seul autre moyen de l'apprendre.
    #[test]
    fn aucune_direction_ne_tombe_sur_un_emplacement_mort() {
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let map = carte();
        let fc = cube::face_chunks(map);
        for j in 0..=180 {
            let lat = FRAC_PI_2 - PI * j as f64 / 180.0;
            let (sin_lat, cos_lat) = lat.sin_cos();
            for i in 0..360 {
                let lon = TAU * i as f64 / 360.0 - PI;
                let (sin_lon, cos_lon) = lon.sin_cos();
                let d = Vec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat);

                let mut face = 0usize;
                let mut meilleur = f64::MIN;
                for f in 0..6 {
                    let n = cube::BASES[f].n;
                    let dot = d.x * n[0] as f64 + d.y * n[1] as f64 + d.z * n[2] as f64;
                    if dot > meilleur {
                        meilleur = dot;
                        face = f;
                    }
                }
                let (col, ligne) = cube::PATRON[face];
                let cpos = Vec2::new(col * fc, ligne * fc);
                assert!(
                    cube::chunk_vivant(map, cpos),
                    "direction ({i}, {j}) mène à un emplacement mort : {cpos:?}",
                );
            }
        }
    }

    /// **L'écran et le monde se répondent.**
    ///
    /// Le pendant, à l'écran, de `projection_inversible` : un pixel du disque
    /// donne une position du monde, qui reprojetée retombe sur le même pixel.
    /// C'est ce qui répond du clic qui pose un marqueur — sans quoi il se
    /// poserait à côté, et d'autant plus loin qu'on serait près du limbe.
    ///
    /// Le limbe exact (`|p| = 1`) est écarté, et ce n'est pas une commodité :
    /// l'orthographie y est **dégénérée**, un pixel y couvre un arc entier et
    /// la reprojection n'y a plus de réponse. C'est pourquoi le code des
    /// marqueurs l'efface au lieu d'y empiler des sites.
    #[test]
    fn le_disque_et_le_monde_se_repondent() {
        let map = carte();
        let mut pire: f64 = 0.0;
        for (lon, lat) in [
            (0.0, 0.0),
            (1.3, 0.7),
            (-2.4, -1.1),
            (0.4, 1.5),
            (3.0, -0.2),
        ] {
            let vue = Vue::depuis_angles(lon, lat);
            for a in -99..=99 {
                for b in -99..=99 {
                    let p = Vec2::new(a as f64, b as f64) / 100.0;
                    if p.magnitude_squared() >= 1.0 {
                        continue;
                    }
                    let d = vue.depuis_ecran(p).expect("dans le disque");
                    let lieu = cube::depuis_direction(map, d);
                    let retour = cube::direction_de(map, lieu);
                    let q = vue.projeter(retour).expect("le point reste visible");
                    pire = pire.max((q - p).magnitude());
                }
            }
        }
        // Mesuré à 3·10⁻¹² ; la borne laisse six ordres de marge, de quoi
        // attraper une régression franche — un `f32` glissé quelque part — sans
        // se casser sur du bruit d'arrondi.
        assert!(pire < 1.0e-6, "aller-retour du disque : {pire}");
    }

    /// Le repère de vue reste orthonormé partout, y compris posé sur les
    /// tangentes non orthogonales d'un coin de face.
    #[test]
    fn le_repere_de_vue_reste_droit() {
        let map = carte();
        let f = cube::face_blocs(map) as f64;
        for face in 0..6u8 {
            for (u, v) in [
                (0.5, 0.5),
                (f - 0.5, 0.5),
                (f / 2.0, f / 2.0),
                (1.0, f - 1.0),
            ] {
                for cap in [0.0, 1.0, -2.5] {
                    let vue = Vue::depuis_lieu(map, cube::Lieu { face, u, v }, cap);
                    for (a, b) in [
                        (vue.droite, vue.haut),
                        (vue.haut, vue.avant),
                        (vue.avant, vue.droite),
                    ] {
                        assert!(a.dot(b).abs() < 1.0e-9, "repère non orthogonal");
                        assert!((a.magnitude() - 1.0).abs() < 1.0e-9, "repère non unitaire");
                    }
                }
            }
        }
    }
}
