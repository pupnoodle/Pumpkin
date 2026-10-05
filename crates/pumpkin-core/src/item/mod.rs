pub mod items;
pub mod potion;
pub mod registry;

use std::any::Any;
use std::sync::Arc;

use crate::block::registry::BlockActionResult;
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::server::Server;
use pumpkin_data::Block;
use pumpkin_data::BlockDirection;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_util::GameMode;
use pumpkin_util::Hand;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

pub trait ItemMetadata {
    fn ids() -> Box<[u16]>;
}

pub trait ItemBehaviour: Send + Sync {
    fn normal_use(&self, _item: &Item, _player: &Player) {}

    /// Handles an item use with the rotation reported for that action.
    ///
    /// Java clients include this rotation in the use-item packet. Item behaviours
    /// that perform a raycast should override this method instead of relying on
    /// the player's potentially stale entity rotation.
    fn normal_use_with_rotation(&self, item: &Item, player: &Player, _yaw: f32, _pitch: f32) {
        self.normal_use(item, player);
    }

    /// Handles an item use with the rotation and the hand reported for that
    /// action, where [`Hand::Right`] is the main hand.
    ///
    /// Defaults to [`Self::normal_use_with_rotation`] so item behaviours that do
    /// not care about the hand keep working unchanged.
    fn normal_use_with_hand(
        &self,
        item: &Item,
        player: &Player,
        yaw: f32,
        pitch: f32,
        _hand: Hand,
    ) {
        self.normal_use_with_rotation(item, player, yaw, pitch);
    }

    #[expect(clippy::too_many_arguments)]
    fn use_on_block(
        &self,
        _item: &mut ItemStack,
        _player: &Player,
        _location: BlockPos,
        _face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        _block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        BlockActionResult::Pass
    }

    fn use_on_entity(&self, _item: &mut ItemStack, _player: &Player, _entity: Arc<dyn EntityBase>) {
    }

    fn on_stopped_using(&self, _stack: &ItemStack, _player: &Player) {}

    fn on_spear_jab(&self, _stack: &ItemStack, _player: &Player) {}

    fn on_use_tick(&self, _stack: &ItemStack, _player: &Player, _remaining_use_ticks: i32) {}

    /// Returns the maximum number of ticks this item can be used for.
    /// Return 0 if the item does not have a behaviour-driven use duration.
    fn get_use_duration(&self) -> i32 {
        0
    }

    fn can_mine(&self, _player: &Player) -> bool {
        true
    }

    fn get_start_and_end_pos(&self, player: &Player) -> (Vector3<f64>, Vector3<f64>) {
        let start_pos = player.eye_position();
        let (yaw, pitch) = player.rotation();
        let (yaw_rad, pitch_rad) = (f64::from(yaw.to_radians()), f64::from(pitch.to_radians()));
        let block_interaction_range = 4.5; // This is not the same as the block_interaction_range in the
        // player entity.
        let direction = Vector3::new(
            -yaw_rad.sin() * pitch_rad.cos() * block_interaction_range,
            -pitch_rad.sin() * block_interaction_range,
            pitch_rad.cos() * yaw_rad.cos() * block_interaction_range,
        );

        let end_pos = start_pos.add(&direction);
        (start_pos, end_pos)
    }

    fn as_any(&self) -> &dyn Any;
}

pub(crate) fn slot_index_for_hand(hand: Hand, selected_hotbar: u8) -> usize {
    match hand {
        Hand::Right => selected_hotbar as usize,
        Hand::Left => PlayerInventory::OFF_HAND_SLOT,
    }
}

pub(crate) fn decrement_placed_stack(gamemode: GameMode, stack: &mut ItemStack) {
    if gamemode != GameMode::Creative {
        stack.decrement(1);
    }
}

pub(crate) fn stored_stack_after_use(
    before: &ItemStack,
    mut after: ItemStack,
) -> Option<ItemStack> {
    if after.is_empty() {
        after.clear();
    }
    if after.are_equal(before) {
        None
    } else {
        Some(after)
    }
}

pub(crate) fn store_used_hand(
    inventory: &PlayerInventory,
    hand: Hand,
    before: &ItemStack,
    after: ItemStack,
) -> Option<ItemStack> {
    let updated = stored_stack_after_use(before, after)?;
    inventory.set_stack_in_hand(hand, updated.clone());
    Some(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;
    use pumpkin_inventory::build_equipment_slots;
    use pumpkin_inventory::entity_equipment::EntityEquipment;
    use std::sync::{Arc, Mutex};

    fn inventory() -> PlayerInventory {
        PlayerInventory::new(
            Arc::new(Mutex::new(EntityEquipment::new())),
            Arc::new(build_equipment_slots()),
        )
    }

    fn place_from_main_hand(item: &'static Item, count: u8) -> PlayerInventory {
        let inventory = inventory();
        inventory.set_held_item(ItemStack::new(count, item));
        let hand = match Hand::from_packet_id(0) {
            Ok(hand) => hand,
            Err(_) => panic!("packet hand 0 is the main hand"),
        };
        let mut stack = inventory.get_stack_in_hand(hand);
        let before = stack.clone();
        decrement_placed_stack(GameMode::Survival, &mut stack);
        let slot = slot_index_for_hand(hand, inventory.get_selected_slot());
        assert_ne!(slot, PlayerInventory::OFF_HAND_SLOT);
        store_used_hand(&inventory, hand, &before, stack);
        inventory
    }

    #[test]
    fn survival_place_decrements_the_held_stack_only() {
        for item in [&Item::OAK_LOG, &Item::SWEET_BERRIES, &Item::GLOW_BERRIES] {
            let inventory = place_from_main_hand(item, 2);
            let held = inventory.held_item();
            assert_eq!(held.item.id, item.id);
            assert_eq!(held.item_count, 1);
            assert!(inventory.off_hand_item().is_empty());
        }
    }

    #[test]
    fn creative_place_does_not_consume_or_copy_the_stack() {
        let inventory = inventory();
        inventory.set_held_item(ItemStack::new(4, &Item::OAK_LOG));
        let hand = match Hand::from_packet_id(0) {
            Ok(hand) => hand,
            Err(_) => panic!("packet hand 0 is the main hand"),
        };
        let mut stack = inventory.get_stack_in_hand(hand);
        let before = stack.clone();
        decrement_placed_stack(GameMode::Creative, &mut stack);
        assert!(store_used_hand(&inventory, hand, &before, stack).is_none());
        assert_eq!(inventory.held_item().item_count, 4);
        assert!(inventory.off_hand_item().is_empty());
    }

    #[test]
    fn offhand_place_does_not_change_the_main_hand() {
        let inventory = inventory();
        inventory.set_held_item(ItemStack::new(5, &Item::DIRT));
        inventory.set_stack_in_hand(Hand::Left, ItemStack::new(3, &Item::OAK_LOG));
        let hand = match Hand::from_packet_id(1) {
            Ok(hand) => hand,
            Err(_) => panic!("packet hand 1 is the off hand"),
        };
        assert!(matches!(hand, Hand::Left));
        let mut stack = inventory.get_stack_in_hand(hand);
        let before = stack.clone();
        decrement_placed_stack(GameMode::Survival, &mut stack);
        store_used_hand(&inventory, hand, &before, stack);
        assert_eq!(inventory.held_item().item_count, 5);
        assert_eq!(inventory.held_item().item.id, Item::DIRT.id);
        assert_eq!(inventory.off_hand_item().item_count, 2);
        assert_eq!(inventory.off_hand_item().item.id, Item::OAK_LOG.id);
    }
}
