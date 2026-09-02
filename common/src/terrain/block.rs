use super::{
    SpriteKind,
    sprite::{self, RelativeNeighborPosition},
};
use crate::{
    comp::{fluid_dynamics::LiquidKind, tool::ToolKind},
    consts::FRIC_GROUND,
    effect::BuffEffect,
    make_case_elim, rtsim,
    vol::FilledVox,
};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use strum::{Display, EnumIter, EnumString};
use vek::*;

make_case_elim!(
    block_kind,
    #[derive(
        Copy,
        Clone,
        Debug,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize,
        FromPrimitive,
        EnumString,
        EnumIter,
        Display,
    )]
    #[repr(u8)]
    pub enum BlockKind {
        Air = 0x00, // Air counts as a fluid
        Water = 0x01,
        // 0x02 <= x < 0x10 are reserved for other fluids. These are 2^n aligned to allow bitwise
        // checking of common conditions. For example, `is_fluid` is just `block_kind &
        // 0x0F == 0` (this is a very common operation used in meshing that could do with
        // being *very* fast).
        Rock = 0x10,
        WeakRock = 0x11, // Explodable
        Lava = 0x12,     // TODO: Reevaluate whether this should be in the rock section
        GlowingRock = 0x13,
        GlowingWeakRock = 0x14,
        // Les deux roches dures de l'echelle des grades : la pierre dure ne cede
        // qu'a un outil de pierre, l'obsidienne qu'a un outil de metal.
        HardRock = 0x15,
        Obsidian = 0x16,
        // Les deux roches des regions extremes (D43) : le basalte des mers de
        // lave, le cristal des zones de magie instable.
        Basalt = 0x17,
        Crystal = 0x18,
        // Le gres des deserts. Il existait deja comme *apparence* — un
        // `WeakRock` teinte dans `world/src/layer/rock.rs` —, ce qui donnait
        // deux gres : l'un lachait de la pierre, l'autre du gres.
        Sandstone = 0x19,
        // 0x1A <= x < 0x20 is reserved for future rocks
        Grass = 0x20, // Note: *not* the same as grass sprites
        Snow = 0x21,
        // Snow to use with sites, to not attract snowfall particles
        ArtSnow = 0x22,
        // 0x21 <= x < 0x30 is reserved for future grasses
        Earth = 0x30,
        Sand = 0x31,
        // Les trois sols des regions extremes (D43) : la cendre volcanique, la
        // tourbe des marais miasmiques, le sable vitrifie par la foudre.
        Ash = 0x32,
        Peat = 0x33,
        Fulgurite = 0x34,
        // La pierraille des pentes de montagne : de la roche **deliee**, d'ou
        // sa place chez les sols et non chez les roches.
        Scree = 0x35,
        // 0x36 <= x < 0x40 is reserved for future earths/muds/gravels/sands/etc.
        Wood = 0x40,
        Leaves = 0x41,
        GlowingMushroom = 0x42,
        Ice = 0x43,
        ArtLeaves = 0x44,
        // Les deux glaces des calottes (D42) et le bois pourri des marais.
        // `Ice` occupait deja cette plage en amont, malgre son intitule : les
        // glaces se rangent aupres d'elle plutot que de se disperser.
        PackIce = 0x45,
        ShelfIce = 0x46,
        Blight = 0x47,
        // 0x48 <= x < 0x50 is reserved for future tree parts
        // Covers all other cases (we sometimes have bizarrely coloured misc blocks, and also we
        // often want to experiment with new kinds of block without allocating them a
        // dedicated block kind.
        Misc = 0xFE,
    }
);

impl BlockKind {
    #[inline]
    pub const fn is_air(&self) -> bool { matches!(self, BlockKind::Air) }

    /// Determine whether the block kind is a gas or a liquid. This does not
    /// consider any sprites that may occupy the block (the definition of
    /// fluid is 'a substance that deforms to fit containers')
    #[inline]
    pub const fn is_fluid(&self) -> bool { *self as u8 & 0xF0 == 0x00 }

    #[inline]
    pub const fn is_liquid(&self) -> bool { self.is_fluid() && !self.is_air() }

    #[inline]
    pub const fn liquid_kind(&self) -> Option<LiquidKind> {
        Some(match self {
            BlockKind::Water => LiquidKind::Water,
            BlockKind::Lava => LiquidKind::Lava,
            _ => return None,
        })
    }

    /// Determine whether the block is filled (i.e: fully solid). Right now,
    /// this is the opposite of being a fluid.
    #[inline]
    pub const fn is_filled(&self) -> bool { !self.is_fluid() }

    /// Determine whether the block has an RGB color stored in the attribute
    /// fields.
    #[inline]
    pub const fn has_color(&self) -> bool { self.is_filled() }

    /// Determine whether the block is 'terrain-like'. This definition is
    /// arbitrary, but includes things like rocks, soils, sands, grass, and
    /// other blocks that might be expected to the landscape. Plant matter and
    /// snow are *not* included.
    #[inline]
    pub const fn is_terrain(&self) -> bool {
        matches!(
            self,
            BlockKind::Rock
                | BlockKind::WeakRock
                | BlockKind::GlowingRock
                | BlockKind::GlowingWeakRock
                | BlockKind::HardRock
                | BlockKind::Basalt
                | BlockKind::Crystal
                | BlockKind::Sandstone
                | BlockKind::Grass
                | BlockKind::Earth
                | BlockKind::Sand
                | BlockKind::Ash
                | BlockKind::Peat
                | BlockKind::Fulgurite
                | BlockKind::Scree
        )
    }

    /// L'objet lâché quand un bloc de ce type est cassé, s'il y en a un.
    ///
    /// Volontairement défini sur le seul `BlockKind` : la couleur du bloc n'est
    /// pas conservée, donc reposer ce qu'on a ramassé ne rend pas la teinte
    /// d'origine. C'est un choix, pas un oubli — porter la couleur demanderait
    /// un objet par teinte, or il y en a seize millions par matériau.
    ///
    /// Les fluides et `Misc` ne lâchent rien : le premier n'a rien à lâcher, le
    /// second est la soupape `0xFE`, employée par la génération de sites pour
    /// des couleurs arbitraires, et son contenu n'a aucun sens de matériau.
    pub const fn item_drop_asset(&self) -> Option<&'static str> {
        Some(match self {
            BlockKind::Rock
            | BlockKind::WeakRock
            | BlockKind::GlowingRock
            | BlockKind::GlowingWeakRock => "common.items.block.stone",
            BlockKind::HardRock => "common.items.block.hard_rock",
            BlockKind::Obsidian => "common.items.block.obsidian",
            BlockKind::Sandstone => "common.items.block.sandstone",
            BlockKind::Scree => "common.items.block.scree",
            BlockKind::Basalt => "common.items.block.basalt",
            BlockKind::Crystal => "common.items.block.crystal",
            BlockKind::Ash => "common.items.block.ash",
            BlockKind::Peat => "common.items.block.peat",
            BlockKind::Fulgurite => "common.items.block.fulgurite",
            BlockKind::PackIce => "common.items.block.pack_ice",
            BlockKind::ShelfIce => "common.items.block.shelf_ice",
            BlockKind::Blight => "common.items.block.blight",
            BlockKind::Grass => "common.items.block.grass",
            BlockKind::Snow | BlockKind::ArtSnow => "common.items.block.snow",
            BlockKind::Earth => "common.items.block.earth",
            BlockKind::Sand => "common.items.block.sand",
            BlockKind::Wood => "common.items.block.wood",
            BlockKind::Leaves | BlockKind::ArtLeaves => "common.items.block.leaves",
            BlockKind::Ice => "common.items.block.ice",
            BlockKind::Air
            | BlockKind::Water
            | BlockKind::Lava
            | BlockKind::GlowingMushroom
            | BlockKind::Misc => return None,
        })
    }

    /// La famille d'outil qui creuse ce bloc a son bon tempo.
    ///
    /// A ne pas confondre avec [`Block::mine_tool`], qui reste le verrou du
    /// minage a l'ability — les sprites-minerais, qu'aucune autre famille ne
    /// touche. Ici la famille ne verrouille rien : creuser a la hache une
    /// paroi de roche marche, seulement au tempo de la main nue. C'est le
    /// *grade* qui verrouille, et lui seul.
    ///
    /// `None` signifie qu'il n'y a rien a creuser — les fluides, et eux seuls.
    pub const fn outil_de_casse(&self) -> Option<ToolKind> {
        Some(match self {
            BlockKind::Rock
            | BlockKind::WeakRock
            | BlockKind::GlowingRock
            | BlockKind::GlowingWeakRock
            | BlockKind::HardRock
            | BlockKind::Obsidian
            | BlockKind::Basalt
            | BlockKind::Crystal
            | BlockKind::Sandstone
            | BlockKind::Ice
            | BlockKind::PackIce
            | BlockKind::ShelfIce
            | BlockKind::Misc => ToolKind::Pick,
            BlockKind::Wood
            | BlockKind::Leaves
            | BlockKind::ArtLeaves
            | BlockKind::Blight
            | BlockKind::GlowingMushroom => ToolKind::Axe,
            BlockKind::Earth
            | BlockKind::Sand
            | BlockKind::Grass
            | BlockKind::Snow
            | BlockKind::ArtSnow
            | BlockKind::Ash
            | BlockKind::Peat
            | BlockKind::Fulgurite
            | BlockKind::Scree => ToolKind::Shovel,
            BlockKind::Air | BlockKind::Water | BlockKind::Lava => return None,
        })
    }

    /// Le temps, en secondes, qu'une main nue met a venir a bout de ce bloc.
    ///
    /// C'est le numerateur de la duree ; le denominateur est la vitesse de
    /// l'outil. Voir [`crate::creusement::temps_de_casse`], qui les assemble et
    /// qui est le seul endroit ou la duree se decide.
    pub const fn durete(&self) -> f32 {
        match self {
            BlockKind::Grass
            | BlockKind::Snow
            | BlockKind::ArtSnow
            | BlockKind::Leaves
            | BlockKind::ArtLeaves
            | BlockKind::GlowingMushroom => 0.4,
            BlockKind::Earth | BlockKind::Sand => 0.6,
            BlockKind::Ash | BlockKind::Peat => 0.5,
            BlockKind::Blight => 1.5,
            // De la roche, mais deliee : on la deplace, on ne la casse pas.
            BlockKind::Scree => 1.2,
            BlockKind::Fulgurite => 1.6,
            BlockKind::Wood | BlockKind::Ice => 2.0,
            // La banquise est broyee et fracturee, la barriere est du neve
            // tasse : la seconde resiste plus que la premiere.
            BlockKind::PackIce => 2.0,
            BlockKind::ShelfIce => 3.0,
            BlockKind::Rock
            | BlockKind::WeakRock
            | BlockKind::GlowingRock
            | BlockKind::GlowingWeakRock
            | BlockKind::Misc => 5.0,
            BlockKind::Sandstone => 3.0,
            BlockKind::Basalt => 8.0,
            BlockKind::Crystal => 10.0,
            BlockKind::HardRock => 12.0,
            BlockKind::Obsidian => 30.0,
            BlockKind::Air | BlockKind::Water | BlockKind::Lava => 0.0,
        }
    }

    /// Le grade d'outil au-dessous duquel le bloc casse **sans rien lacher**.
    ///
    /// Le bloc part quand meme : c'est la perte qui enseigne, la ou un bloc qui
    /// resiste n'enseigne rien. Voir [`crate::creusement::grade_outil`] pour
    /// l'echelle, empruntee a `MaterialKind`.
    pub const fn grade_requis(&self) -> u8 {
        match self {
            BlockKind::Grass
            | BlockKind::Snow
            | BlockKind::ArtSnow
            | BlockKind::Leaves
            | BlockKind::ArtLeaves
            | BlockKind::GlowingMushroom
            | BlockKind::Earth
            | BlockKind::Sand
            | BlockKind::Ash
            | BlockKind::Peat
            | BlockKind::Blight
            | BlockKind::Scree
            | BlockKind::Air
            | BlockKind::Water
            | BlockKind::Lava => 0,
            BlockKind::Wood
            | BlockKind::Ice
            | BlockKind::Rock
            | BlockKind::WeakRock
            | BlockKind::GlowingRock
            | BlockKind::GlowingWeakRock
            | BlockKind::Fulgurite
            | BlockKind::Sandstone
            | BlockKind::PackIce
            | BlockKind::ShelfIce
            | BlockKind::Misc => 1,
            // Le basalte et le cristal sont des roches de region extreme : on
            // ne les rapporte pas avec une pioche de bois.
            BlockKind::HardRock | BlockKind::Basalt | BlockKind::Crystal => 2,
            BlockKind::Obsidian => 3,
        }
    }
}

/// Le bloc que pose un objet-bloc, s'il en pose un.
///
/// Inverse de [`BlockKind::item_drop_asset`], et **seul endroit ou se decide
/// ce qu'un joueur pose** : le client n'envoie qu'un emplacement d'inventaire,
/// le serveur lit l'objet qui s'y trouve et vient ici.
///
/// Deux choses que l'objet ne porte pas et qu'il faut donc choisir :
///
/// - **le type**, quand plusieurs partagent un meme objet — la roche friable et
///   la roche luisante lachent toutes deux « un bloc de pierre ». On rend le
///   type canonique de la famille, jamais une de ses variantes ;
/// - **la teinte**, perdue au ramassage. Ce sont les couleurs des modeles
///   d'objets, premiere entree de palette de `gen_block_vox.py`, si bien que ce
///   qu'on pose ressemble a ce qu'on tient. Le gazon fait exception et prend le
///   vert de son dessus, pas la terre de ses flancs.
pub fn block_from_item(item_id: &str) -> Option<Block> {
    let (kind, (r, g, b)) = match item_id {
        "common.items.block.stone" => (BlockKind::Rock, (122, 122, 128)),
        "common.items.block.hard_rock" => (BlockKind::HardRock, (78, 80, 88)),
        "common.items.block.obsidian" => (BlockKind::Obsidian, (34, 28, 44)),
        "common.items.block.sandstone" => (BlockKind::Sandstone, (204, 166, 110)),
        "common.items.block.scree" => (BlockKind::Scree, (122, 118, 112)),
        "common.items.block.basalt" => (BlockKind::Basalt, (56, 52, 58)),
        "common.items.block.crystal" => (BlockKind::Crystal, (150, 116, 214)),
        "common.items.block.ash" => (BlockKind::Ash, (92, 86, 84)),
        "common.items.block.peat" => (BlockKind::Peat, (66, 54, 40)),
        "common.items.block.fulgurite" => (BlockKind::Fulgurite, (196, 186, 214)),
        "common.items.block.pack_ice" => (BlockKind::PackIce, (176, 206, 220)),
        "common.items.block.shelf_ice" => (BlockKind::ShelfIce, (222, 234, 246)),
        "common.items.block.blight" => (BlockKind::Blight, (72, 66, 48)),
        "common.items.block.grass" => (BlockKind::Grass, (74, 132, 54)),
        "common.items.block.snow" => (BlockKind::Snow, (238, 242, 248)),
        "common.items.block.earth" => (BlockKind::Earth, (104, 74, 50)),
        "common.items.block.sand" => (BlockKind::Sand, (214, 194, 140)),
        "common.items.block.wood" => (BlockKind::Wood, (126, 92, 56)),
        "common.items.block.leaves" => (BlockKind::Leaves, (64, 116, 46)),
        "common.items.block.ice" => (BlockKind::Ice, (168, 208, 226)),
        _ => return None,
    };
    Some(Block::new(kind, Rgb::new(r, g, b)))
}

/// # Format
///
/// ```ignore
/// BBBBBBBB CCCCCCCC AAAAAIII IIIIIIII
/// ```
/// - `0..8`  : BlockKind
/// - `8..16` : Category
/// - `16..N` : Attributes (many fields)
/// - `N..32` : Sprite ID
///
/// `N` is per-category. You can match on the category byte to find the length
/// of the ID field.
///
/// Attributes are also per-category. Each category specifies its own list of
/// attribute fields.
///
/// Why is the sprite ID at the end? Simply put, it makes masking faster and
/// easier, which is important because extracting the `SpriteKind` is a more
/// commonly performed operation than extracting attributes.
#[derive(Copy, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Block {
    kind: BlockKind,
    data: [u8; 3],
}

impl std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Block");

        s.field("kind", &self.kind);

        if let Some(sprite) = super::StructureSprite::from_block(self) {
            s.field("sprite", &sprite);
        }

        if self.is_filled() {
            s.field("color", &self.data);
        }

        s.finish()
    }
}

impl FilledVox for Block {
    fn default_non_filled() -> Self { Block::air(SpriteKind::Empty) }

    fn is_filled(&self) -> bool { self.kind.is_filled() }
}

impl Deref for Block {
    type Target = BlockKind;

    fn deref(&self) -> &Self::Target { &self.kind }
}

impl Block {
    pub const MAX_HEIGHT: f32 = 3.0;

    /* Constructors */

    #[inline]
    pub const fn from_raw(kind: BlockKind, data: [u8; 3]) -> Self { Self { kind, data } }

    // TODO: Rename to `filled`, make caller guarantees stronger
    #[inline]
    #[track_caller]
    pub const fn new(kind: BlockKind, color: Rgb<u8>) -> Self {
        if kind.is_filled() {
            Self::from_raw(kind, [color.r, color.g, color.b])
        } else {
            // Works because `SpriteKind::Empty` has no attributes
            let data = (SpriteKind::Empty as u32).to_be_bytes();
            Self::from_raw(kind, [data[1], data[2], data[3]])
        }
    }

    // Only valid if `block_kind` is unfilled, so this is just a private utility
    // method
    #[inline]
    pub fn unfilled(kind: BlockKind, sprite: SpriteKind) -> Self {
        #[cfg(debug_assertions)]
        assert!(!kind.is_filled());

        Self::from_raw(kind, sprite.to_initial_bytes())
    }

    #[inline]
    pub fn air(sprite: SpriteKind) -> Self { Self::unfilled(BlockKind::Air, sprite) }

    #[inline]
    pub const fn empty() -> Self {
        // Works because `SpriteKind::Empty` has no attributes
        let data = (SpriteKind::Empty as u32).to_be_bytes();
        Self::from_raw(BlockKind::Air, [data[1], data[2], data[3]])
    }

    #[inline]
    pub fn water(sprite: SpriteKind) -> Self { Self::unfilled(BlockKind::Water, sprite) }

    /* Sprite decoding */

    #[inline(always)]
    pub const fn get_sprite(&self) -> Option<SpriteKind> {
        if !self.kind.is_filled() {
            SpriteKind::from_block(*self)
        } else {
            None
        }
    }

    #[inline(always)]
    pub(super) const fn sprite_category_byte(&self) -> u8 { self.data[0] }

    #[inline(always)]
    pub const fn sprite_category(&self) -> Option<sprite::Category> {
        if self.kind.is_filled() {
            None
        } else {
            sprite::Category::from_block(*self)
        }
    }

    /// Build this block with the given sprite attribute set.
    #[inline]
    pub fn with_attr<A: sprite::Attribute>(
        mut self,
        attr: A,
    ) -> Result<Self, sprite::AttributeError<core::convert::Infallible>> {
        self.set_attr(attr)?;
        Ok(self)
    }

    /// Set the given attribute of this block's sprite.
    #[inline]
    pub fn set_attr<A: sprite::Attribute>(
        &mut self,
        attr: A,
    ) -> Result<(), sprite::AttributeError<core::convert::Infallible>> {
        match self.sprite_category() {
            Some(category) => category.write_attr(self, attr),
            None => Err(sprite::AttributeError::NotPresent),
        }
    }

    /// Get the given attribute of this block's sprite.
    #[inline]
    pub fn get_attr<A: sprite::Attribute>(&self) -> Result<A, sprite::AttributeError<A::Error>> {
        match self.sprite_category() {
            Some(category) => category.read_attr(*self),
            None => Err(sprite::AttributeError::NotPresent),
        }
    }

    pub fn rotation_mat(&self) -> Mat3<i32> {
        let dir = crate::util::Dir2::from_sprite_ori(
            self.get_attr::<sprite::Ori>().unwrap_or_default().0,
        )
        .map(|(d, _)| d)
        .unwrap_or(crate::util::Dir2::X);

        let mut rot_mat = dir.to_mat3();

        rot_mat.cols *= self.sprite_mirror_vec().map(|f| f as i32);

        rot_mat
    }

    pub fn sprite_z_rot(&self) -> Option<f32> {
        self.get_attr::<sprite::Ori>()
            .ok()
            .map(|ori| std::f32::consts::PI * 0.25 * ori.0 as f32)
    }

    pub fn sprite_mirror_vec(&self) -> Vec3<f32> {
        Vec3::new(
            self.get_attr::<sprite::MirrorX>().map(|m| m.0),
            self.get_attr::<sprite::MirrorY>().map(|m| m.0),
            self.get_attr::<sprite::MirrorZ>().map(|m| m.0),
        )
        .map(|b| match b.unwrap_or(false) {
            true => -1.0,
            false => 1.0,
        })
    }

    #[inline(always)]
    pub(super) const fn data(&self) -> [u8; 3] { self.data }

    #[inline(always)]
    pub(super) const fn with_data(mut self, data: [u8; 3]) -> Self {
        self.data = data;
        self
    }

    #[inline(always)]
    pub(super) const fn to_be_u32(self) -> u32 {
        u32::from_be_bytes([self.kind as u8, self.data[0], self.data[1], self.data[2]])
    }

    #[inline]
    pub fn get_color(&self) -> Option<Rgb<u8>> {
        if self.has_color() {
            Some(self.data.into())
        } else {
            None
        }
    }

    /// Returns the rtsim resource, if any, that this block corresponds to. If
    /// you want the scarcity of a block to change with rtsim's resource
    /// depletion tracking, you can do so by editing this function.
    // TODO: Return type should be `Option<&'static [(rtsim::TerrainResource, f32)]>` to allow
    // fractional quantities and multiple resources per sprite
    #[inline]
    pub fn get_rtsim_resource(&self) -> Option<rtsim::TerrainResource> {
        match self.get_sprite()? {
            SpriteKind::Stones | SpriteKind::Stones2 => Some(rtsim::TerrainResource::Stone),
            SpriteKind::Twigs
            | SpriteKind::Wood
            | SpriteKind::Bamboo
            | SpriteKind::Hardwood
            | SpriteKind::Ironwood
            | SpriteKind::Frostwood
            | SpriteKind::Eldwood => Some(rtsim::TerrainResource::Wood),
            SpriteKind::Amethyst
            | SpriteKind::Ruby
            | SpriteKind::Sapphire
            | SpriteKind::Emerald
            | SpriteKind::Topaz
            | SpriteKind::Diamond
            | SpriteKind::CrystalHigh
            | SpriteKind::CrystalLow
            | SpriteKind::Lodestone => Some(rtsim::TerrainResource::Gem),
            SpriteKind::Bloodstone
            | SpriteKind::Coal
            | SpriteKind::Cobalt
            | SpriteKind::Copper
            | SpriteKind::Iron
            | SpriteKind::Tin
            | SpriteKind::Silver
            | SpriteKind::Gold => Some(rtsim::TerrainResource::Ore),
            SpriteKind::LongGrass
            | SpriteKind::MediumGrass
            | SpriteKind::ShortGrass
            | SpriteKind::LargeGrass
            | SpriteKind::GrassBlue
            | SpriteKind::SavannaGrass
            | SpriteKind::TallSavannaGrass
            | SpriteKind::RedSavannaGrass
            | SpriteKind::JungleRedGrass
            | SpriteKind::Fern => Some(rtsim::TerrainResource::Grass),
            SpriteKind::BlueFlower
            | SpriteKind::PinkFlower
            | SpriteKind::PurpleFlower
            | SpriteKind::RedFlower
            | SpriteKind::WhiteFlower
            | SpriteKind::YellowFlower
            | SpriteKind::Sunflower
            | SpriteKind::Moonbell
            | SpriteKind::Pyrebloom => Some(rtsim::TerrainResource::Flower),
            SpriteKind::Reed
            | SpriteKind::Flax
            | SpriteKind::WildFlax
            | SpriteKind::Cotton
            | SpriteKind::Corn
            | SpriteKind::WheatYellow
            | SpriteKind::WheatGreen => Some(rtsim::TerrainResource::Plant),
            SpriteKind::Apple
            | SpriteKind::Pumpkin
            | SpriteKind::Beehive // TODO: Not a fruit, but kind of acts like one
            | SpriteKind::Coconut => Some(rtsim::TerrainResource::Fruit),
            SpriteKind::Lettuce
            | SpriteKind::Carrot
            | SpriteKind::Tomato
            | SpriteKind::Radish
            | SpriteKind::Turnip => Some(rtsim::TerrainResource::Vegetable),
            SpriteKind::Mushroom
            | SpriteKind::CaveMushroom
            | SpriteKind::CeilingMushroom
            | SpriteKind::RockyMushroom
            | SpriteKind::LushMushroom
            | SpriteKind::GlowMushroom => Some(rtsim::TerrainResource::Mushroom),
            // Catch all for other things that give items, but aren't specified above.
            s if s.default_loot_spec().is_some_and(|inner| inner.is_some()) => Some(rtsim::TerrainResource::Loot),
            _ => None,
        }
        // Don't count collected sprites.
        // TODO: we may want to have rtsim still spawn these sprites when depleted by spawning them
        // in the "collected" state, see `into_collected` for sprites that would need this.
        .filter(|_|  matches!(self.get_attr(), Ok(sprite::Collectable(true)) | Err(_)))
    }

    #[inline]
    pub fn get_glow(&self) -> Option<u8> {
        let glow_level = match self.kind() {
            BlockKind::Lava => 24,
            BlockKind::GlowingRock | BlockKind::GlowingWeakRock => 10,
            // Le cristal arcanique luit faiblement : assez pour qu'une pastille
            // de magie instable se repere de loin, pas assez pour eclairer.
            BlockKind::Crystal => 6,
            BlockKind::GlowingMushroom => 20,
            _ => match self.get_sprite()? {
                SpriteKind::StreetLamp | SpriteKind::StreetLampTall | SpriteKind::BonfireMLit => 24,
                SpriteKind::Ember | SpriteKind::FireBlock => 20,
                SpriteKind::WallLamp
                | SpriteKind::WallLampSmall
                | SpriteKind::WallLampWizard
                | SpriteKind::WallLampMesa
                | SpriteKind::WallSconce
                | SpriteKind::FireBowlGround
                | SpriteKind::MesaLantern
                | SpriteKind::LampTerracotta
                | SpriteKind::ChristmasOrnament
                | SpriteKind::CliffDecorBlock
                | SpriteKind::Orb
                | SpriteKind::Candle => 16,
                SpriteKind::DiamondLight => 30,
                SpriteKind::VeloriteFrag
                | SpriteKind::GrassBlueShort
                | SpriteKind::GrassBlueMedium
                | SpriteKind::GrassBlueLong
                | SpriteKind::CavernLillypadBlue
                | SpriteKind::MycelBlue
                | SpriteKind::Mold
                | SpriteKind::CeilingMushroom => 6,
                SpriteKind::CaveMushroom
                | SpriteKind::GlowMushroom
                | SpriteKind::CookingPot
                | SpriteKind::CrystalHigh
                | SpriteKind::LanternFlower
                | SpriteKind::CeilingLanternFlower
                | SpriteKind::LanternPlant
                | SpriteKind::CeilingLanternPlant
                | SpriteKind::CrystalLow => 10,
                SpriteKind::SewerMushroom => 16,
                SpriteKind::Lodestone => 3,
                SpriteKind::Lantern
                | SpriteKind::LanternpostWoodLantern
                | SpriteKind::LanternAirshipWallBlackS
                | SpriteKind::LanternAirshipWallBrownS
                | SpriteKind::LanternAirshipWallChestnutS
                | SpriteKind::LanternAirshipWallRedS
                | SpriteKind::LanternAirshipGroundBlackS
                | SpriteKind::LanternAirshipGroundBrownS
                | SpriteKind::LanternAirshipGroundChestnutS
                | SpriteKind::LanternAirshipGroundRedS
                | SpriteKind::LampMetalShinglesCyan
                | SpriteKind::LampMetalShinglesRed => 24,
                SpriteKind::Velorite | SpriteKind::TerracottaStatue => 8,
                SpriteKind::SeashellLantern | SpriteKind::GlowIceCrystal => 16,
                SpriteKind::SeaDecorEmblem => 12,
                SpriteKind::SeaDecorBlock
                | SpriteKind::HaniwaKeyDoor
                | SpriteKind::VampireKeyDoor => 10,
                _ => return None,
            },
        };

        if self
            .get_attr::<sprite::LightEnabled>()
            .map_or(true, |l| l.0)
        {
            Some(glow_level)
        } else {
            None
        }
    }

    // minimum block, attenuation
    #[inline]
    pub fn get_max_sunlight(&self) -> (u8, f32) {
        match self.kind() {
            BlockKind::Water => (0, 0.4),
            BlockKind::Leaves => (9, 255.0),
            BlockKind::ArtLeaves => (9, 255.0),
            BlockKind::Wood => (6, 2.0),
            BlockKind::Snow => (6, 2.0),
            BlockKind::ArtSnow => (6, 2.0),
            BlockKind::Ice => (4, 2.0),
            _ if self.is_opaque() => (0, 255.0),
            _ => (0, 0.0),
        }
    }

    // Filled blocks or sprites
    #[inline]
    pub fn is_solid(&self) -> bool {
        self.get_sprite()
            .map(|s| s.solid_height().is_some())
            .unwrap_or(!matches!(self.kind, BlockKind::Lava))
    }

    pub fn valid_collision_dir(
        &self,
        entity_aabb: Aabb<f32>,
        block_aabb: Aabb<f32>,
        move_dir: Vec3<f32>,
    ) -> bool {
        self.get_sprite().is_none_or(|sprite| {
            sprite.valid_collision_dir(entity_aabb, block_aabb, move_dir, self)
        })
    }

    /// Can this block be exploded? If so, what 'power' is required to do so?
    /// Note that we don't really define what 'power' is. Consider the units
    /// arbitrary and only important when compared to one-another.
    #[inline]
    pub fn explode_power(&self) -> Option<f32> {
        // Explodable means that the terrain sprite will get removed anyway,
        // so all is good for empty fluids.
        match self.kind() {
            BlockKind::Leaves => Some(0.25),
            BlockKind::ArtLeaves => Some(0.25),
            BlockKind::Grass => Some(0.5),
            BlockKind::WeakRock => Some(0.75),
            BlockKind::Snow => Some(0.1),
            BlockKind::Ice => Some(0.5),
            BlockKind::Wood => Some(4.5),
            BlockKind::Lava => None,
            _ => self.get_sprite().and_then(|sprite| match sprite {
                sprite if sprite.is_defined_as_container() => None,
                SpriteKind::Keyhole
                | SpriteKind::KeyDoor
                | SpriteKind::BoneKeyhole
                | SpriteKind::BoneKeyDoor
                | SpriteKind::OneWayWall
                | SpriteKind::KeyholeBars
                | SpriteKind::DoorBars => None,
                SpriteKind::Anvil
                | SpriteKind::Cauldron
                | SpriteKind::CookingPot
                | SpriteKind::CraftingBench
                | SpriteKind::Forge
                | SpriteKind::Loom
                | SpriteKind::SpinningWheel
                | SpriteKind::DismantlingBench
                | SpriteKind::RepairBench
                | SpriteKind::TanningRack
                | SpriteKind::Chest
                | SpriteKind::DungeonChest0
                | SpriteKind::DungeonChest1
                | SpriteKind::DungeonChest2
                | SpriteKind::DungeonChest3
                | SpriteKind::DungeonChest4
                | SpriteKind::DungeonChest5
                | SpriteKind::CoralChest
                | SpriteKind::HaniwaUrn
                | SpriteKind::HaniwaKeyDoor
                | SpriteKind::HaniwaKeyhole
                | SpriteKind::VampireKeyDoor
                | SpriteKind::VampireKeyhole
                | SpriteKind::MyrmidonKeyDoor
                | SpriteKind::MyrmidonKeyhole
                | SpriteKind::MinotaurKeyhole
                | SpriteKind::HaniwaTrap
                | SpriteKind::HaniwaTrapTriggered
                | SpriteKind::ChestBuried
                | SpriteKind::CommonLockedChest
                | SpriteKind::TerracottaChest
                | SpriteKind::SahaginChest
                | SpriteKind::SeaDecorBlock
                | SpriteKind::SeaDecorChain
                | SpriteKind::SeaDecorWindowHor
                | SpriteKind::SeaDecorWindowVer
                | SpriteKind::WitchWindow
                | SpriteKind::Rope
                | SpriteKind::MetalChain
                | SpriteKind::IronSpike
                | SpriteKind::HotSurface
                | SpriteKind::FireBlock
                | SpriteKind::GlassBarrier
                | SpriteKind::GlassKeyhole
                | SpriteKind::SahaginKeyhole
                | SpriteKind::SahaginKeyDoor
                | SpriteKind::TerracottaKeyDoor
                | SpriteKind::TerracottaKeyhole
                | SpriteKind::TerracottaStatue
                | SpriteKind::TerracottaBlock => None,
                SpriteKind::EnsnaringVines
                | SpriteKind::EnsnaringWeb
                | SpriteKind::SeaUrchin
                | SpriteKind::IceSpike
                | SpriteKind::DiamondLight => Some(0.1),
                _ => Some(0.25),
            }),
        }
    }

    /// Whether the block containes a sprite that is collectible.
    ///
    /// Note, this is based on [`SpriteKind::collectible_info`] and accounts for
    /// if the [`Collectable`][`sprite::Collectable`] sprite attr is `false`.
    #[inline]
    pub fn is_collectible(&self) -> bool {
        self.get_sprite()
            .is_some_and(|s| s.collectible_info().is_some())
            && matches!(self.get_attr(), Ok(sprite::Collectable(true)) | Err(_))
    }

    /// Can this sprite be picked up to yield an item without a tool?
    ///
    /// Note, this is based on [`SpriteKind::collectible_info`] and accounts for
    /// if the [`Collectable`][`sprite::Collectable`] sprite attr is `false`.
    #[inline]
    pub fn is_directly_collectible(&self) -> bool {
        // NOTE: This doesn't require `SpriteCfg` because `SpriteCfg::loot_table` is
        // only expected to be set for `collectible_info.is_some()` sprites!
        self.get_sprite()
            .is_some_and(|s| s.collectible_info() == Some(None))
            && matches!(self.get_attr(), Ok(sprite::Collectable(true)) | Err(_))
    }

    #[inline]
    pub fn is_mountable(&self) -> bool { self.mount_offset().is_some() }

    /// Get the position and direction to mount this block if any.
    pub fn mount_offset(&self) -> Option<(Vec3<f32>, Vec3<f32>)> {
        self.get_sprite().and_then(|sprite| sprite.mount_offset())
    }

    pub fn mount_buffs(&self) -> Option<Vec<BuffEffect>> {
        self.get_sprite().and_then(|sprite| sprite.mount_buffs())
    }

    pub fn is_controller(&self) -> bool {
        self.get_sprite()
            .is_some_and(|sprite| sprite.is_controller())
    }

    #[inline]
    pub fn is_bonkable(&self) -> bool {
        match self.get_sprite() {
            Some(
                SpriteKind::Apple | SpriteKind::Beehive | SpriteKind::Coconut | SpriteKind::Bomb,
            ) => self.is_solid(),
            _ => false,
        }
    }

    #[inline]
    pub fn is_owned(&self) -> bool {
        self.get_attr::<sprite::Owned>()
            .is_ok_and(|sprite::Owned(b)| b)
    }

    /// The tool required to mine this block. For blocks that cannot be mined,
    /// `None` is returned.
    #[inline]
    pub fn mine_tool(&self) -> Option<ToolKind> {
        match self.kind() {
            BlockKind::WeakRock | BlockKind::Ice | BlockKind::GlowingWeakRock => {
                Some(ToolKind::Pick)
            },
            _ => self.get_sprite().and_then(|s| s.mine_tool()),
        }
    }

    #[inline]
    pub fn is_opaque(&self) -> bool {
        match self.get_sprite() {
            Some(
                SpriteKind::Keyhole
                | SpriteKind::KeyDoor
                | SpriteKind::KeyholeBars
                | SpriteKind::DoorBars,
            ) => true,
            Some(_) => false,
            None => self.kind().is_filled(),
        }
    }

    #[inline]
    pub fn solid_height(&self) -> f32 {
        self.get_sprite()
            .map(|s| s.solid_height().unwrap_or(0.0))
            .unwrap_or(1.0)
    }

    /// Get the friction constant used to calculate surface friction when
    /// walking/climbing. Currently has no units.
    #[inline]
    pub fn get_friction(&self) -> f32 {
        match self.kind() {
            BlockKind::Ice => FRIC_GROUND * 0.1,
            _ => FRIC_GROUND,
        }
    }

    /// Get the traction permitted by this block as a proportion of the friction
    /// applied.
    ///
    /// 1.0 = default, 0.0 = completely inhibits movement, > 1.0 = potential for
    /// infinite acceleration (in a vacuum).
    #[inline]
    pub fn get_traction(&self) -> f32 {
        match self.kind() {
            BlockKind::Snow | BlockKind::ArtSnow => 0.8,
            _ => 1.0,
        }
    }

    /// Apply a light toggle to this block, if possible
    pub fn with_toggle_light(self, enable: bool) -> Option<Self> {
        self.with_attr(sprite::LightEnabled(enable)).ok()
    }

    #[inline]
    pub fn kind(&self) -> BlockKind { self.kind }

    /// If possible, copy the sprite/color data of the other block.
    #[inline]
    #[must_use]
    pub fn with_data_of(mut self, other: Block) -> Self {
        if self.is_filled() == other.is_filled() {
            self = self.with_data(other.data());
        }
        self
    }

    /// If this block is a fluid, replace its sprite.
    #[inline]
    #[must_use]
    pub fn with_sprite(self, sprite: SpriteKind) -> Self {
        match self.try_with_sprite(sprite) {
            Ok(b) => b,
            Err(b) => b,
        }
    }

    /// If this block is a fluid, replace its sprite.
    ///
    /// Returns block in `Err` if the sprite was not replaced.
    #[inline]
    pub fn try_with_sprite(self, sprite: SpriteKind) -> Result<Self, Self> {
        if self.is_filled() {
            Err(self)
        } else {
            Ok(Self::unfilled(self.kind, sprite))
        }
    }

    /// If this block can have orientation, give it a new orientation.
    #[inline]
    #[must_use]
    pub fn with_ori(self, ori: u8) -> Option<Self> { self.with_attr(sprite::Ori(ori)).ok() }

    /// If this block can have adjacent sprites, give it its AdjacentType
    #[inline]
    #[must_use]
    pub fn with_adjacent_type(self, adj: RelativeNeighborPosition) -> Option<Self> {
        self.with_attr(sprite::AdjacentType(adj as u8)).ok()
    }

    /// Remove the terrain sprite or solid aspects of a block
    #[inline]
    #[must_use]
    pub fn into_vacant(self) -> Self {
        if self.is_fluid() {
            Block::unfilled(self.kind(), SpriteKind::Empty)
        } else {
            // FIXME: Figure out if there's some sensible way to determine what medium to
            // replace a filled block with if it's removed.
            Block::air(SpriteKind::Empty)
        }
    }

    /// Apply the effect of collecting the sprite in this block.
    ///
    /// This sets the `Collectable` attribute to `false` for some sprites like
    /// `Lettuce`. Other sprites will simply be removed via
    /// [`into_vacant`][Self::into_vacant].
    #[inline]
    #[must_use]
    pub fn into_collected(self) -> Self {
        match self.get_sprite() {
            Some(SpriteKind::Lettuce) => self.with_attr(sprite::Collectable(false)).expect(
                "Setting collectable will not fail since this sprite has Collectable attribute",
            ),
            _ => self.into_vacant(),
        }
    }

    /// Attempt to convert a [`u32`] to a block
    #[inline]
    #[must_use]
    pub fn from_u32(x: u32) -> Option<Self> {
        let [bk, r, g, b] = x.to_le_bytes();
        let block = Self {
            kind: BlockKind::from_u8(bk)?,
            data: [r, g, b],
        };

        (block.kind.is_filled() || SpriteKind::from_block(block).is_some()).then_some(block)
    }

    #[inline]
    pub fn to_u32(self) -> u32 {
        u32::from_le_bytes([self.kind as u8, self.data[0], self.data[1], self.data[2]])
    }
}

const _: () = assert!(core::mem::size_of::<BlockKind>() == 1);
const _: () = assert!(core::mem::size_of::<Block>() == 4);

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// Les deux tables se repondent. C'est la seule garantie qu'elles ne
    /// divergeront pas : casser un bloc puis reposer ce qu'on a ramasse doit
    /// rendre un bloc qui lache a nouveau le meme objet.
    #[test]
    fn drop_and_place_agree() {
        for kind in BlockKind::iter() {
            let Some(asset) = kind.item_drop_asset() else {
                continue;
            };
            let placed = block_from_item(asset)
                .unwrap_or_else(|| panic!("{kind:?} lache {asset}, qu'aucun bloc ne repose"));
            assert_eq!(
                placed.kind().item_drop_asset(),
                Some(asset),
                "{kind:?} lache {asset}, repose {:?}, qui lache autre chose",
                placed.kind(),
            );
        }
    }

    /// Un bloc pose porte bien sa teinte : les huit materiaux sont pleins, donc
    /// leurs trois octets de donnees sont une couleur et non un sprite.
    #[test]
    fn placed_block_carries_its_colour() {
        let earth = block_from_item("common.items.block.earth").expect("la terre se pose");
        assert_eq!(earth.get_color(), Some(Rgb::new(104, 74, 50)));
    }
}
