//! L'orientation d'une entite.
//!
//! `Ori` lui-meme est dans `common-vocab` : il est bati sur `Dir`, avec lequel
//! il forme un cycle, et `comp::body` en a besoin. Ne reste ici que ce qui
//! nomme des composants du jeu.

pub use common_vocab::geometrie::ori::Ori;

use crate::util::Dir;
use vek::Vec3;

/// La direction dans laquelle une entite regarde une autre, hauteur des yeux
/// comprise.
///
/// Etait `Dir::look_toward`, dans `util::dir`, ou elle faisait remonter `util`
/// vers `comp` pour son seul usage.
pub fn look_toward(
    this_pos: &super::Pos,
    this_body: Option<&super::Body>,
    this_scale: Option<&super::Scale>,
    tgt_pos: &super::Pos,
    tgt_body: Option<&super::Body>,
    tgt_scale: Option<&super::Scale>,
) -> Option<Dir> {
    let eye_offset = this_body.map_or(0.0, |b| b.eye_height(this_scale.map_or(1.0, |s| s.0)));
    let tgt_eye_offset = tgt_body.map_or(0.0, |b| b.eye_height(tgt_scale.map_or(1.0, |s| s.0)));
    Dir::from_unnormalized(
        Vec3::new(tgt_pos.0.x, tgt_pos.0.y, tgt_pos.0.z + tgt_eye_offset)
            - Vec3::new(this_pos.0.x, this_pos.0.y, this_pos.0.z + eye_offset),
    )
}

