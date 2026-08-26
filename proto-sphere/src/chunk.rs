//! Un chunk : 32×32 blocs en `u` et `v`, toute la profondeur `Z` d'un coup.
//!
//! Les chunks sont générés avec une marge d'un bloc. Le maillage peut alors
//! décider seul si une face est visible, sans jamais consulter un chunk
//! voisin — et comme la marge passe par `replier_bloc`, un chunk au bord d'une
//! face voit correctement ce qu'il y a sur la face d'à côté, fût-elle tournée
//! d'un quart de tour.

use crate::monde::{Bloc, Generateur, HAUTEUR_CHUNK, NIVEAU_MER, TAILLE_CHUNK};

pub const MARGE: i32 = 1;
pub const LARGEUR: i32 = TAILLE_CHUNK + 2 * MARGE;

pub struct Chunk {
    blocs: Vec<Bloc>,
    /// Altitude maximale rencontrée : borne la boucle de maillage.
    pub sommet: i32,
}

impl Chunk {
    pub fn generer(gen: &Generateur, face: u8, cu: i32, cv: i32) -> Self {
        let mut blocs = vec![Bloc::Air; (LARGEUR * LARGEUR * HAUTEUR_CHUNK) as usize];
        let mut sommet = 0;

        for lv in -MARGE..TAILLE_CHUNK + MARGE {
            for lu in -MARGE..TAILLE_CHUNK + MARGE {
                let (sol, biome) =
                    gen.colonne(face, cu * TAILLE_CHUNK + lu, cv * TAILLE_CHUNK + lv);
                sommet = sommet.max(sol.max(NIVEAU_MER));
                for z in 0..HAUTEUR_CHUNK {
                    let b = gen.bloc(sol, biome, z);
                    if b != Bloc::Air {
                        blocs[indice(lu, lv, z)] = b;
                    }
                }
            }
        }

        Self { blocs, sommet }
    }

    #[inline]
    pub fn bloc(&self, lu: i32, lv: i32, z: i32) -> Bloc {
        if z < 0 {
            return Bloc::Roche;
        }
        if z >= HAUTEUR_CHUNK
            || lu < -MARGE
            || lv < -MARGE
            || lu >= TAILLE_CHUNK + MARGE
            || lv >= TAILLE_CHUNK + MARGE
        {
            return Bloc::Air;
        }
        self.blocs[indice(lu, lv, z)]
    }
}

#[inline]
fn indice(lu: i32, lv: i32, z: i32) -> usize {
    let x = (lu + MARGE) as usize;
    let y = (lv + MARGE) as usize;
    (z as usize * (LARGEUR * LARGEUR) as usize) + y * LARGEUR as usize + x
}
