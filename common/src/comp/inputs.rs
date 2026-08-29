use serde::{Deserialize, Serialize};
use specs::{Component, DenseVecStorage, DerefFlaggedStorage};

/// Le mode construction, porte par tout personnage.
///
/// Il n'a plus de zones : tout point du monde est constructible, et la seule
/// borne est la portee du joueur (`MAX_BUILD_RANGE`). Ce qui reste ici est un
/// mode, pas une permission — il decide si les touches `1`-`0` selectionnent la
/// matiere au lieu de l'employer, et si le reticule vise un bloc.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanBuild {
    pub enabled: bool,
}
impl Component for CanBuild {
    type Storage = DerefFlaggedStorage<Self, DenseVecStorage<Self>>;
}
