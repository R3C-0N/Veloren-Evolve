use serde::{Deserialize, Serialize};
use specs::{Component, DenseVecStorage, DerefFlaggedStorage};

/// Le mode de jeu du personnage : aventure, ou combat.
///
/// **L'aventure est le mode normal**, et c'est D7 au pied de la lettre — le
/// present est paisible, on y creuse et on y batit sans rien degainer. Le
/// combat ne s'y substitue que le temps d'une empoignade : il rend les clics
/// aux armes, ne laisse que trois cases de matiere pour barricader, et interdit
/// de creuser.
///
/// On y entre en **etant frappe** (`EntityAttackedHookEvent`) ou par la touche,
/// on en sort par la touche seule. Pas de minuteur : c'est le joueur qui juge
/// que c'est fini, et se tromper ne coute qu'un coup encaisse — le suivant l'y
/// ramene.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeDeJeu {
    pub combat: bool,
}

impl ModeDeJeu {
    pub fn aventure(&self) -> bool { !self.combat }
}

impl Component for ModeDeJeu {
    type Storage = DerefFlaggedStorage<Self, DenseVecStorage<Self>>;
}
