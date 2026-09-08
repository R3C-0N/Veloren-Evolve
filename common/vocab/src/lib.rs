#![feature(fundamental)]

//! Le vocabulaire partage des crates de `common`.
//!
//! Ce que contient cette crate n'a aucune dependance vers le reste du jeu : ce
//! sont des types et des macros dont plusieurs modules ont besoin, et dont la
//! presence dans `veloren-common` obligeait ceux qui les utilisent a dependre
//! de tout le reste.

pub mod consts;
pub mod typed;
