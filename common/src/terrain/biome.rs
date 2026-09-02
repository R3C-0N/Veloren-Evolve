use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// Les sept engins de D19, et donc les sept regions qu'ils ouvrent.
///
/// Un palier n'est pas un rang : D15 laisse leur ordre libre, et D23 ne compte
/// que leur *nombre*. C'est une porte, pas un echelon.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, EnumIter)]
pub enum Palier {
    SousMarin,
    Aeronef,
    Foreuse,
    TrainSki,
    BateauObsidienne,
    ScaphandreAnticorrosion,
    SphereDeStase,
}

#[derive(
    Default, Debug, Copy, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Hash, EnumIter,
)]
pub enum BiomeKind {
    #[default]
    Void,
    Lake,
    Grassland,
    Ocean,
    Mountain,
    Snowland,
    Desert,
    Swamp,
    Jungle,
    Forest,
    Savannah,
    Taiga,
    // Les cinq extremes de D19 qui sont des biomes de surface (D40). Les deux
    // autres regions ne peuvent pas en etre : les cieux n'ont pas de colonne,
    // et les profondeurs souterraines ont deja leurs biomes ponderes dans
    // `world/src/layer/cave.rs`.
    /// Les profondeurs sous-marines — sous la banquise comme au large.
    Abyss,
    /// La banquise du nord : petite, fracturee, escarpee.
    PackIce,
    /// La barriere de glace du sud : vaste, plate, a front de falaise.
    IceShelf,
    /// Les mers de lave, dans les chaines de montagnes.
    Volcanic,
    /// Les marais miasmiques.
    Miasma,
    /// Les zones de magie instable — la foudre.
    Arcane,
}

impl BiomeKind {
    /// Roughly represents the difficulty of a biome (value between 1 and 5)
    ///
    /// Les cinq extremes saturent l'echelle : elle n'a que cinq crans, et ils
    /// sont tous au-dela de ce qu'elle sait dire. Elle ne sert qu'au placement
    /// de faune, ou cette approximation ne coute rien.
    pub fn difficulty(&self) -> i32 {
        match self {
            BiomeKind::Void => 1,
            BiomeKind::Lake => 1,
            BiomeKind::Grassland => 2,
            BiomeKind::Ocean => 1,
            BiomeKind::Mountain => 1,
            BiomeKind::Snowland => 2,
            BiomeKind::Desert => 5,
            BiomeKind::Swamp => 2,
            BiomeKind::Jungle => 3,
            BiomeKind::Forest => 1,
            BiomeKind::Savannah => 2,
            BiomeKind::Taiga => 2,
            BiomeKind::Abyss
            | BiomeKind::PackIce
            | BiomeKind::IceShelf
            | BiomeKind::Volcanic
            | BiomeKind::Miasma
            | BiomeKind::Arcane => 5,
        }
    }

    /// Le palier technologique qu'il faut avoir bati pour tenir ici (D19).
    ///
    /// **C'est la donnee, pas son application.** Rien n'interdit encore
    /// l'entree a un joueur sans son engin ; ce qui la refusera lira ceci.
    ///
    /// Le nord et le sud partagent le train-ski faute de mieux : Q28 demande si
    /// la banquise, qui flotte sur l'abysse, ne devrait pas etre la premiere
    /// region combinee de D25 — train-ski et sous-marin.
    pub fn palier_requis(&self) -> Option<Palier> {
        Some(match self {
            BiomeKind::Abyss => Palier::SousMarin,
            BiomeKind::PackIce | BiomeKind::IceShelf => Palier::TrainSki,
            BiomeKind::Volcanic => Palier::BateauObsidienne,
            BiomeKind::Miasma => Palier::ScaphandreAnticorrosion,
            BiomeKind::Arcane => Palier::SphereDeStase,
            _ => return None,
        })
    }
}

#[cfg(test)]
#[test]
fn test_biome_difficulty() {
    use strum::IntoEnumIterator;

    for biome_kind in BiomeKind::iter() {
        assert!(
            (1..=5).contains(&biome_kind.difficulty()),
            "Biome {biome_kind:?} has invalid difficulty {}",
            biome_kind.difficulty()
        );
    }
}

/// Six biomes portent un palier, pour cinq regions de D19.
///
/// Six et non cinq : la region arctique se lit en deux biomes, la banquise et
/// la barriere, parce qu'on ne les traverse pas de la meme facon (D42). C'est
/// exactement ce que Q28 met en question.
#[cfg(test)]
#[test]
fn les_extremes_portent_un_palier() {
    use strum::IntoEnumIterator;

    let extremes = BiomeKind::iter()
        .filter(|b| b.palier_requis().is_some())
        .count();
    assert_eq!(extremes, 6, "six biomes a palier, pas {extremes}");

    // Et rien d'ordinaire n'en porte : un biome a palier est une region qu'on
    // doit avoir bati de quoi atteindre.
    assert!(BiomeKind::Grassland.palier_requis().is_none());
    assert!(BiomeKind::Ocean.palier_requis().is_none());
}
