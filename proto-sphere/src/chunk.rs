//! Un chunk : 32x32 blocs en X et Y, toute la profondeur Z d'un coup.
//!
//! Les chunks sont generes avec une marge d'un bloc sur X et Y. Le maillage
//! peut alors decider seul si une face est visible, sans jamais consulter un
//! chunk voisin — et comme la marge passe par [`plier_bloc`], un chunk au bord
//! du monde voit correctement ce qu'il y a de l'autre cote de la couture.

use crate::monde::{Bloc, Generateur, HAUTEUR_CHUNK, TAILLE_CHUNK};

pub const MARGE: i32 = 1;
pub const LARGEUR: i32 = TAILLE_CHUNK + 2 * MARGE;

pub struct Chunk {
    blocs: Vec<Bloc>,
    /// Altitude maximale rencontree : borne la boucle de maillage.
    pub sommet: i32,
}

impl Chunk {
    pub fn generer(gen: &Generateur, cx: i32, cy: i32) -> Self {
        let mut blocs = vec![Bloc::Air; (LARGEUR * LARGEUR * HAUTEUR_CHUNK) as usize];
        let mut sommet = 0;

        for ly in -MARGE..TAILLE_CHUNK + MARGE {
            for lx in -MARGE..TAILLE_CHUNK + MARGE {
                let (sol, biome) = gen.colonne(cx * TAILLE_CHUNK + lx, cy * TAILLE_CHUNK + ly);
                sommet = sommet.max(sol.max(crate::monde::NIVEAU_MER));
                for z in 0..HAUTEUR_CHUNK {
                    let b = gen.bloc(sol, biome, z);
                    if b != Bloc::Air {
                        blocs[indice(lx, ly, z)] = b;
                    }
                }
            }
        }

        Self { blocs, sommet }
    }

    #[inline]
    pub fn bloc(&self, lx: i32, ly: i32, z: i32) -> Bloc {
        if z < 0 {
            return Bloc::Roche;
        }
        if z >= HAUTEUR_CHUNK
            || lx < -MARGE
            || ly < -MARGE
            || lx >= TAILLE_CHUNK + MARGE
            || ly >= TAILLE_CHUNK + MARGE
        {
            return Bloc::Air;
        }
        self.blocs[indice(lx, ly, z)]
    }
}

#[inline]
fn indice(lx: i32, ly: i32, z: i32) -> usize {
    let x = (lx + MARGE) as usize;
    let y = (ly + MARGE) as usize;
    (z as usize * (LARGEUR * LARGEUR) as usize) + y * LARGEUR as usize + x
}
