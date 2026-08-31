//! Poser un objet plat sur la planète (D27).
//!
//! Le terrain et l'eau se projettent **sommet par sommet**, dans le shader :
//! ils s'étendent sur tout un chunk, et la projection y varie sensiblement.
//! Tout le reste — une figure, un sprite, une particule — est petit devant le
//! rayon, et ne subit alors de la projection qu'une transformation **rigide** :
//! la flèche vaut `w²/8r`, soit 6·10⁻⁴ bloc pour cinq blocs sur un rayon de
//! cinq mille. C'est l'argument que D29 employait déjà pour la nappe du
//! portail.
//!
//! D'où : rien à changer dans leurs shaders. Il suffit de composer la matrice
//! de modèle que chacun possède déjà.
//!
//! **Le repère se prend à l'origine de l'objet, jamais par sommet.** Chercher
//! la face sommet par sommet déchirerait en deux tout modèle à cheval sur une
//! couture, ses sommets se répartissant entre deux bases.

use common::terrain::{MapSizeLg, cube};
use vek::*;

/// Ce qu'il faut savoir du monde pour y poser un objet.
#[derive(Copy, Clone, Debug)]
pub struct PoseSpherique {
    map: MapSizeLg,
    /// Le point de convergence, déjà projeté.
    origine: Vec3<f32>,
    /// La partie entière du foyer, que les shaders retirent d'eux-mêmes.
    focus_off: Vec3<f32>,
}

impl PoseSpherique {
    /// `None` sur une carte plate : il n'y a alors rien à poser.
    pub fn nouvelle(map: MapSizeLg, origine: Vec3<f32>, focus_off: Vec3<f32>) -> Option<Self> {
        map.est_cubique().then_some(Self {
            map,
            origine,
            focus_off,
        })
    }

    /// Le repère et la place d'un objet situé en `wpos`.
    fn repere_et_place(&self, wpos: Vec3<f32>) -> Option<(Mat4<f32>, Vec3<f32>)> {
        let m = cube::pose(
            self.map,
            wpos.map(|e| e as f64),
            self.origine.map(|e| e as f64),
        )?;
        let m = m.map(|e| e as f32);
        // La colonne de translation est la place ; le reste est le repère.
        let place = Vec3::new(m[(0, 3)], m[(1, 3)], m[(2, 3)]);
        let mut repere = m;
        repere[(0, 3)] = 0.0;
        repere[(1, 3)] = 0.0;
        repere[(2, 3)] = 0.0;
        Some((repere, place))
    }

    /// Pour les objets dont la rotation et la position voyagent **séparément**
    /// — une figure porte sa matrice de modèle d'un côté, sa position de
    /// l'autre.
    ///
    /// La position rendue est la place projetée **plus** le foyer entier, parce
    /// que le shader retirera celui-ci de lui-même. C'est ce qui évite d'avoir
    /// à le toucher.
    pub fn appliquer(&self, mat: Mat4<f32>, wpos: Vec3<f32>) -> (Mat4<f32>, Vec3<f32>) {
        match self.repere_et_place(wpos) {
            Some((repere, place)) => (repere * mat, place + self.focus_off),
            None => (mat, wpos),
        }
    }

    /// Pour les objets dont la matrice porte **tout**, translation comprise —
    /// un sprite, une particule.
    ///
    /// On lui reprend sa position, on la projette, et on recompose : le reste
    /// de la matrice — rotation, échelle — est laissé intact.
    pub fn appliquer_matrice(&self, mat: Mat4<f32>) -> Mat4<f32> {
        let wpos = Vec3::new(mat[(0, 3)], mat[(1, 3)], mat[(2, 3)]);
        let Some((repere, place)) = self.repere_et_place(wpos) else {
            return mat;
        };
        let mut sans_translation = mat;
        sans_translation[(0, 3)] = 0.0;
        sans_translation[(1, 3)] = 0.0;
        sans_translation[(2, 3)] = 0.0;
        Mat4::<f32>::translation_3d(place + self.focus_off) * repere * sans_translation
    }
}
