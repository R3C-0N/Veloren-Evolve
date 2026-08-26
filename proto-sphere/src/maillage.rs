//! Maillage naif : une face par frontiere entre un bloc plein et de l'air.
//!
//! Pas de greedy meshing. Le prototype juge une topologie et une illusion, pas
//! un budget de sommets.

use crate::chunk::Chunk;
use crate::monde::{HAUTEUR_CHUNK, TAILLE_CHUNK};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Sommet {
    /// Position locale au chunk, en blocs. Le decalage vers la camera est
    /// applique au rendu : le maillage ne connait pas sa place dans le monde.
    pub position: [f32; 3],
    pub couleur: [f32; 3],
}

/// Les six faces : direction du voisin, les quatre coins, et l'eclairement.
const FACES: [([i32; 3], [[f32; 3]; 4], f32); 6] = [
    // dessus
    ([0, 0, 1], [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]], 1.00),
    // dessous
    ([0, 0, -1], [[0., 1., 0.], [1., 1., 0.], [1., 0., 0.], [0., 0., 0.]], 0.42),
    // est / ouest
    ([1, 0, 0], [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]], 0.80),
    ([-1, 0, 0], [[0., 1., 0.], [0., 0., 0.], [0., 0., 1.], [0., 1., 1.]], 0.72),
    // nord / sud
    ([0, 1, 0], [[1., 1., 0.], [0., 1., 0.], [0., 1., 1.], [1., 1., 1.]], 0.64),
    ([0, -1, 0], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]], 0.56),
];

pub fn mailler(chunk: &Chunk) -> (Vec<Sommet>, Vec<u32>) {
    let mut sommets = Vec::new();
    let mut indices = Vec::new();
    let plafond = (chunk.sommet + 2).min(HAUTEUR_CHUNK);

    for z in 0..plafond {
        for ly in 0..TAILLE_CHUNK {
            for lx in 0..TAILLE_CHUNK {
                let bloc = chunk.bloc(lx, ly, z);
                if !bloc.plein() {
                    continue;
                }
                let base = bloc.couleur();

                for (dir, coins, ombre) in FACES.iter() {
                    if chunk.bloc(lx + dir[0], ly + dir[1], z + dir[2]).plein() {
                        continue;
                    }
                    let debut = sommets.len() as u32;
                    let couleur = [base[0] * ombre, base[1] * ombre, base[2] * ombre];
                    for coin in coins.iter() {
                        sommets.push(Sommet {
                            position: [
                                lx as f32 + coin[0],
                                ly as f32 + coin[1],
                                z as f32 + coin[2],
                            ],
                            couleur,
                        });
                    }
                    indices.extend_from_slice(&[
                        debut, debut + 1, debut + 2, debut, debut + 2, debut + 3,
                    ]);
                }
            }
        }
    }

    (sommets, indices)
}
