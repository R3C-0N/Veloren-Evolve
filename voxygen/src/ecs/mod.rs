pub mod comp;
pub mod sys;

use common::slowjob::SlowJobPool;
use specs::{World, WorldExt};

pub fn init(world: &mut World) {
    world.register::<comp::HpFloaterList>();
    world.register::<comp::Interpolated>();
    world.register::<comp::Footsteps>();

    {
        let pool = world.read_resource::<SlowJobPool>();
        pool.configure("IMAGE_PROCESSING", |n| n / 2);
        pool.configure("FIGURE_MESHING", |n| n / 2);
        // Au moins deux, sinon le remaillage est serie.
        //
        // `n` est le `slow_limit` du pool (`common/state/src/state.rs`), qui
        // vaut `cpus/2 + cpus/4` : sur quatre coeurs il fait 3, et `3 / 2`
        // donnait **1**. Un bloc casse remaille jusqu'a neuf chunks a 97 ms
        // piece ; enchaines un par un, cela faisait 800 ms avant que le
        // changement ne se voie. Le garde `meshing_cores` de
        // `scene/terrain/mod.rs` laissait deja passer deux travaux — c'est ici
        // que la file se pincait, et nulle part ailleurs.
        //
        // Le `.max(2)` ne mord que sous huit coeurs : au-dela, `n / 2` est deja
        // plus grand et rien ne bouge.
        pool.configure("TERRAIN_MESHING", |n| (n / 2).max(2));
    }
}
