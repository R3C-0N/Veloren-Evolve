//! Combien de temps casser un bloc coute, et si l'on en recupere quelque chose.
//!
//! Tout passe par ici : le serveur y decide quand le bloc tombe, le client y
//! calcule la jauge qu'il dessine. Les deux appellent [`temps_de_casse`], si
//! bien que la jauge n'a rien a synchroniser — elle part de la meme fonction
//! pure, et le serveur reste seul juge du resultat.
//!
//! L'echelle des grades n'est pas neuve : c'est [`MaterialKind`], la matiere
//! dont l'objet est fait, deja portee par les tags des objets de Veloren. Un
//! outil n'a donc pas de champ « grade » a lui ; il a une matiere, et la
//! matiere est le grade.

use crate::{
    comp::{
        Inventory, Item,
        inventory::{
            item::{ItemDesc, ItemTag, MaterialKind},
            slot::{EquipSlot, FamilleOutil},
        },
    },
    terrain::Block,
};

/// Le grade d'un outil, lu dans sa matiere.
///
/// La main nue vaut 0, et tout ce qui n'est pas fait de bois, de pierre ou de
/// metal aussi : une pioche de gemme ou de cuir ne veut rien dire, et faire
/// semblant de la classer serait pire que de la renvoyer au rang de la main.
pub fn grade_outil(outil: Option<&Item>) -> u8 {
    outil.map_or(0, |item| {
        item.tags()
            .into_iter()
            .filter_map(|tag| match tag {
                ItemTag::MaterialKind(MaterialKind::Wood) => Some(1),
                ItemTag::MaterialKind(MaterialKind::Stone) => Some(2),
                ItemTag::MaterialKind(MaterialKind::Metal) => Some(3),
                _ => None,
            })
            // Un objet peut porter plusieurs matieres — un manche de bois et
            // une tete de metal. C'est la meilleure qui creuse.
            .max()
            .unwrap_or(0)
    })
}

/// La vitesse de creusement, en blocs de durete 1 par seconde.
///
/// Le grade fait le tempo, la famille ne fait qu'un tout ou rien : avec le bon
/// outil on creuse a sa vitesse, avec le mauvais on creuse comme a mains nues.
/// C'est volontairement grossier — un degrade rendrait la hache presque bonne
/// a la pierre, et l'echelle cesserait de se voir.
pub fn vitesse_de_creusement(grade: u8, famille_juste: bool) -> f32 {
    if !famille_juste {
        return 1.0;
    }
    match grade {
        0 => 1.0,
        1 => 2.5,
        2 => 4.0,
        _ => 5.5,
    }
}

/// Le temps, en secondes, pour venir a bout de ce bloc avec cet outil.
///
/// `None` quand il n'y a rien a creuser — un fluide, et rien d'autre.
pub fn temps_de_casse(bloc: &Block, outil: Option<&Item>) -> Option<f32> {
    let famille = bloc.kind().outil_de_casse()?;
    let famille_juste = outil.and_then(|item| item.tool_info()) == Some(famille);
    let vitesse = vitesse_de_creusement(grade_outil(outil), famille_juste);
    Some(bloc.kind().durete() / vitesse)
}

/// L'outil suffit-il a ce que le bloc lache quelque chose ?
///
/// Au-dessous du grade requis le bloc part quand meme, mais rien ne tombe.
pub fn lache_son_objet(bloc: &Block, outil: Option<&Item>) -> bool {
    grade_outil(outil) >= bloc.kind().grade_requis()
}

/// L'outil que le personnage emploie pour ce bloc, s'il en a un.
///
/// **Seule source d'outil du creusement.** Le bloc designe sa famille, on lit
/// l'emplacement correspondant, et ce que le joueur tient en main n'entre
/// jamais en ligne de compte — c'est ce qui evite d'avoir a degainer pour
/// miner. Emplacement vide : on creuse a mains nues, lentement, sans rien
/// ramener de ce qui demande un grade.
pub fn outil_pour<'a>(inventaire: &'a Inventory, bloc: &Block) -> Option<&'a Item> {
    let famille = FamilleOutil::depuis_tool_kind(bloc.kind().outil_de_casse()?)?;
    inventaire.equipped(EquipSlot::Outil(famille))
}
