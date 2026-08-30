//! L'empreinte d'un monde plat, pour l'oracle de non-régression (D37).
//!
//! Le solveur d'érosion fait 2 683 lignes qu'on ne relit pas. La seule preuve
//! solide qu'on ne l'a pas cassé en le rendant cubique, c'est que le monde
//! **plat** sorte identique — même graine, même empreinte.
//!
//! ```bash
//! cargo run --release --example empreinte -- --x-lg 8
//! ```

use common::terrain::uniform_idx_as_vec2;
use std::hash::{DefaultHasher, Hash, Hasher};
use vek::*;
use veloren_world::{
    World,
    sim::{FileOpts, GenOpts, WorldOpts},
};

fn main() {
    let x_lg = std::env::args()
        .skip_while(|a| a != "--x-lg")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8u32);

    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let (monde, _index) = World::generate(
        42,
        WorldOpts {
            seed_elements: true,
            world_file: FileOpts::Generate(GenOpts {
                x_lg,
                y_lg: x_lg,
                ..GenOpts::default()
            }),
            calendar: None,
        },
        &pool,
        &|_| {},
    );

    let sim = monde.sim();
    assert_eq!(sim.map_size_lg().vec(), Vec2::new(x_lg, x_lg));

    // On hache les champs que l'érosion produit, en bits : une moyenne noierait
    // justement l'erreur qu'on cherche.
    let mut h = DefaultHasher::new();
    let mut sommet = 0.0f32;
    for posi in 0..sim.map_size_lg().chunks_len() {
        let pos = uniform_idx_as_vec2(sim.map_size_lg(), posi);
        let c = sim.get(pos).expect("case de la carte");
        c.alt.to_bits().hash(&mut h);
        c.basement.to_bits().hash(&mut h);
        c.water_alt.to_bits().hash(&mut h);
        c.downhill.hash(&mut h);
        sommet = sommet.max(c.alt);
    }

    println!(
        "monde plat {} x {} chunks · graine 42",
        sim.map_size_lg().chunks().x,
        sim.map_size_lg().chunks().y
    );
    println!("altitude maximale : {sommet:.3}");
    println!("empreinte : {:016x}", h.finish());
}
