//! Les corps du jeu.
//!
//! Ce module etait `comp::body`, au milieu de `veloren-common`. Il n'a besoin
//! de rien du jeu : des especes, des morphologies, et les manifestes qui les
//! decrivent. Sa presence la-haut obligeait a compiler tout le reste avant lui.
//!
//! `npc` l'accompagne : `NpcNames` est un `AllBodies`, et `corps::plugin` a
//! besoin de `NpcBody`.

pub mod npc;

mod corps;

pub use corps::*;
