//! A Keyblade item's weapon model. It follows the game's own chain from the
//! equipped item to its weapon object: the item record's
//! +4 is the weapon's position in a WENT set of 03system (counting the set's
//! size word as 0), the set is the one Sora's
//! object entry names by its neo-moveset index, and the set entry is the
//! weapon's object id, whose model name is `obj/<name>.mdlx` (layouts per
//! OpenKH's 03system and 00objentry documentation; checked on the install by
//! the Kingdom Key resolving to the room's own weapon model).
use data::{item_table, objentry::ObjectEntry, weapon_entry};
use std::collections::HashMap;

/// Item category of a Keyblade.
const KEYBLADE: u8 = 2;

/// Keyblade item id -> its weapon object's model name.
#[derive(Default)]
pub struct Keyblades {
    by_item: HashMap<u16, String>,
    /// Model name -> its object entry's motion set name.
    animations: HashMap<String, String>,
}

impl Keyblades {
    /// Reads 03system's WENT and ITEM entries (`went`, `items`) for the
    /// player object `player` among `objects`.
    pub fn read(
        went: &[u8],
        items: &[u8],
        objects: &[ObjectEntry],
        player: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let player = objects
            .iter()
            .find(|o| o.model_name == player.as_bytes())
            .ok_or("player object entry absent")?;
        let set = weapon_entry::set(went, player.neo_moveset as usize)?;
        let mut by_item = HashMap::new();
        for item in item_table::read(items)?
            .iter()
            .filter(|i| i.kind == KEYBLADE)
        {
            // The id counts words from the set's start, the size word being
            // 0 (on the install the Kingdom Key then resolves to W_EX010,
            // the room's own weapon model).
            let Some(object_id) = weapon_entry::object_id(&set, item.amount) else {
                continue;
            };
            if let Some(o) = objects.iter().find(|o| o.object_id == object_id) {
                by_item.insert(item.id, String::from_utf8_lossy(&o.model_name).into_owned());
            }
        }
        let animations = by_item
            .values()
            .filter_map(|name| {
                let o = objects.iter().find(|o| o.model_name == name.as_bytes())?;
                (!o.animation_name.is_empty()).then(|| {
                    (
                        name.clone(),
                        String::from_utf8_lossy(&o.animation_name).into_owned(),
                    )
                })
            })
            .collect();
        Ok(Self {
            by_item,
            animations,
        })
    }

    /// The motion set named by the object entry of model `model`.
    pub fn animation(&self, model: &str) -> Option<&str> {
        self.animations.get(model).map(String::as_str)
    }

    /// The model name of the Keyblade item `item`.
    pub fn model(&self, item: u16) -> Option<&str> {
        self.by_item.get(&item).map(String::as_str)
    }
}
