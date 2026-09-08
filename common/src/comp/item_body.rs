//! Le corps que prend un objet lorsqu'il tombe au sol.
//!
//! C'est un pont entre les objets et les corps. Il etait ecrit du cote des
//! corps, sous forme de deux `impl From<&Item>`, ce qui obligeait
//! `comp::body::item` a connaitre `Item`, `ItemKind`, `Tool` et `ArmorKind`.
//!
//! La regle d'orphelin permettait de le mettre d'un cote ou de l'autre ; il est
//! au-dessus des deux, la ou un pont a sa place.

use crate::comp::{
    body::item::{Body as ItemBody, ItemArmorKind},
    inventory::item::{
        Item, ItemKind, ThrownItem, Utility,
        armor::ArmorKind,
        tool::Tool,
    },
};

pub fn item_body(item: &Item) -> ItemBody {
        match &*item.kind() {
            ItemKind::Tool(Tool { kind, .. }) => ItemBody::Tool(*kind),
            ItemKind::ModularComponent(_) => ItemBody::ModularComponent,
            ItemKind::Lantern(_) => ItemBody::Lantern,
            ItemKind::Glider => ItemBody::Glider,
            ItemKind::Armor(armor) => match armor.kind {
                ArmorKind::Shoulder => ItemBody::Armor(ItemArmorKind::Shoulder),
                ArmorKind::Chest => ItemBody::Armor(ItemArmorKind::Chest),
                ArmorKind::Belt => ItemBody::Armor(ItemArmorKind::Belt),
                ArmorKind::Hand => ItemBody::Armor(ItemArmorKind::Hand),
                ArmorKind::Pants => ItemBody::Armor(ItemArmorKind::Pants),
                ArmorKind::Foot => ItemBody::Armor(ItemArmorKind::Foot),
                ArmorKind::Back => ItemBody::Armor(ItemArmorKind::Back),
                ArmorKind::Backpack => ItemBody::Armor(ItemArmorKind::Back),
                ArmorKind::Ring => ItemBody::Armor(ItemArmorKind::Ring),
                ArmorKind::Neck => ItemBody::Armor(ItemArmorKind::Neck),
                ArmorKind::Head => ItemBody::Armor(ItemArmorKind::Head),
                ArmorKind::Tabard => ItemBody::Armor(ItemArmorKind::Tabard),
                ArmorKind::Bag => ItemBody::Armor(ItemArmorKind::Bag),
            },
            ItemKind::Utility { kind, .. } => match kind {
                Utility::Coins => {
                    if item.amount() > 100 {
                        ItemBody::CoinPouch
                    } else {
                        ItemBody::Coins
                    }
                },
                _ => ItemBody::Utility,
            },
            ItemKind::Consumable { .. } => ItemBody::Consumable,
            ItemKind::Ingredient { .. } => ItemBody::Ingredient,
            _ => ItemBody::Empty,
        }
}

pub fn thrown_item_body(thrown_item: &ThrownItem) -> ItemBody {
        match &*thrown_item.0.kind() {
            ItemKind::Tool(Tool { kind, .. }) => ItemBody::Thrown(*kind),
            _ => ItemBody::Empty,
        }
}

