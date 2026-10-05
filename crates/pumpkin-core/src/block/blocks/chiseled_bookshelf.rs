use std::sync::{Arc, atomic::Ordering};

use pumpkin_macros::pumpkin_block;

use crate::block::entities::chiseled_bookshelf::ChiseledBookshelfBlockEntity;
use crate::{
    block::{
        BlockBehaviour, BlockHitResult, GetComparatorOutputArgs, NormalUseArgs, OnPlaceArgs,
        PlacedArgs, UseWithItemArgs, registry::BlockActionResult,
    },
    entity::{EntityBase, player::Player},
    world::World,
};
use pumpkin_data::{
    BlockStateId,
    block_properties::{ChiseledBookshelfLikeProperties, HorizontalFacing},
    item::Item,
    item_stack::ItemStack,
    sound::{Sound, SoundCategory},
    tag,
    tag::Taggable,
};
use pumpkin_inventory::screen_handler::InventoryPlayer;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

#[pumpkin_block("minecraft:chiseled_bookshelf")]
pub struct ChiseledBookshelfBlock;

impl BlockBehaviour for ChiseledBookshelfBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut properties = ChiseledBookshelfLikeProperties::default(args.block);

        // Face in the opposite direction the player is facing
        properties.facing = args.player.get_entity().get_horizontal_facing().opposite();

        properties.to_state_id(args.block)
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        let state = args.world.get_block_state(args.position);
        let properties = ChiseledBookshelfLikeProperties::from_state_id(state.id);

        if let Some(slot) = Self::get_slot_for_hit(args.hit, properties.facing) {
            if Self::is_slot_used(properties, slot) {
                if let Some(block_entity) = args.world.get_block_entity(args.position)
                    && let Some(block_entity) = block_entity
                        .as_any()
                        .downcast_ref::<ChiseledBookshelfBlockEntity>()
                {
                    Self::try_remove_book(
                        args.world,
                        args.player,
                        args.position,
                        block_entity,
                        properties,
                        slot,
                    );
                    return BlockActionResult::Success;
                }
            } else {
                return BlockActionResult::Consume;
            }
        }
        BlockActionResult::Pass
    }

    fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        let state = args.world.get_block_state(args.position);
        let properties = ChiseledBookshelfLikeProperties::from_state_id(state.id);

        if !args
            .item_stack
            .get_item()
            .has_tag(&tag::Item::MINECRAFT_BOOKSHELF_BOOKS)
        {
            return BlockActionResult::PassToDefaultBlockAction;
        }
        if let Some(slot) = Self::get_slot_for_hit(args.hit, properties.facing) {
            if Self::is_slot_used(properties, slot) {
                return BlockActionResult::PassToDefaultBlockAction;
            } else if let Some(block_entity) = args.world.get_block_entity(args.position)
                && let Some(block_entity) = block_entity
                    .as_any()
                    .downcast_ref::<ChiseledBookshelfBlockEntity>()
            {
                Self::try_add_book(
                    args.world,
                    args.player,
                    args.position,
                    block_entity,
                    properties,
                    slot,
                    args.item_stack,
                );
                return BlockActionResult::Success;
            }
        }

        BlockActionResult::Pass
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let block_entity = ChiseledBookshelfBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(block_entity));
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(block_entity) = block_entity
                .as_any()
                .downcast_ref::<ChiseledBookshelfBlockEntity>()
        {
            return Some((block_entity.last_interacted_slot.load(Ordering::Relaxed) + 1) as u8);
        }
        None
    }
}

impl ChiseledBookshelfBlock {
    fn try_add_book(
        world: &Arc<World>,
        player: &Player,
        position: &BlockPos,
        entity: &ChiseledBookshelfBlockEntity,
        properties: ChiseledBookshelfLikeProperties,
        slot: i8,
        item: &mut ItemStack,
    ) {
        // TODO: Increment used stats for chiseled bookshelf on the player

        let sound = if item.get_item() == &Item::ENCHANTED_BOOK {
            Sound::BlockChiseledBookshelfPickupEnchanted
        } else {
            Sound::BlockChiseledBookshelfPickup
        };

        entity.set_book(
            slot as usize,
            item.split_unless_creative(player.gamemode.load(), 1),
        );
        entity.update_state(properties, world, slot as usize);

        world.play_sound(sound, SoundCategory::Blocks, &position.to_centered_f64());
    }

    fn try_remove_book(
        world: &Arc<World>,
        player: &Arc<Player>,
        position: &BlockPos,
        entity: &ChiseledBookshelfBlockEntity,
        properties: ChiseledBookshelfLikeProperties,
        slot: i8,
    ) {
        let mut stack = entity.remove_book(slot as usize, 1);

        let sound = if stack.get_item() == &Item::ENCHANTED_BOOK {
            Sound::BlockChiseledBookshelfPickupEnchanted
        } else {
            Sound::BlockChiseledBookshelfPickup
        };

        if !player.get_inventory().insert_stack_anywhere(&mut stack) {
            // Drop the item on the ground if the player cannot hold it because of a full inventory
            player.drop_item(stack);
        }
        entity.update_state(properties, world, slot as usize);

        world.play_sound(sound, SoundCategory::Blocks, &position.to_centered_f64());
    }

    fn get_slot_for_hit(hit: &BlockHitResult<'_>, facing: HorizontalFacing) -> Option<i8> {
        Self::get_hit_pos(hit, facing).map(|position| {
            let i = i8::from(position.y < 0.5);
            let j = Self::get_column(position.x);
            j + i * 3
        })
    }

    fn get_hit_pos(hit: &BlockHitResult<'_>, facing: HorizontalFacing) -> Option<Vector2<f32>> {
        // If the direction is not horizontal, we cannot hit a slot
        let direction = hit.face.to_horizontal_facing()?;

        // If the facing direction does not match the block's facing, we cannot hit a slot
        if facing != direction {
            return None;
        }

        match direction {
            HorizontalFacing::North => Some(Vector2::new(1.0 - hit.cursor_pos.x, hit.cursor_pos.y)),
            HorizontalFacing::South => Some(Vector2::new(hit.cursor_pos.x, hit.cursor_pos.y)),
            HorizontalFacing::West => Some(Vector2::new(hit.cursor_pos.z, hit.cursor_pos.y)),
            HorizontalFacing::East => Some(Vector2::new(1.0 - hit.cursor_pos.z, hit.cursor_pos.y)),
        }
    }

    // Magic numbers for the slots
    // These are based on the vanilla chiseled bookshelf implementation
    const OFFSET_SLOT_0: f32 = 0.375;
    const OFFSET_SLOT_1: f32 = 0.6875;

    fn get_column(x: f32) -> i8 {
        if x < Self::OFFSET_SLOT_0 {
            0
        } else if x < Self::OFFSET_SLOT_1 {
            1
        } else {
            2
        }
    }

    const fn is_slot_used(properties: ChiseledBookshelfLikeProperties, slot: i8) -> bool {
        match slot {
            0 => properties.slot_0_occupied,
            1 => properties.slot_1_occupied,
            2 => properties.slot_2_occupied,
            3 => properties.slot_3_occupied,
            4 => properties.slot_4_occupied,
            5 => properties.slot_5_occupied,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::block::entities::chiseled_bookshelf::ChiseledBookshelfBlockEntity;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_inventory::Inventory;
    use pumpkin_inventory::build_equipment_slots;
    use pumpkin_inventory::entity_equipment::EntityEquipment;
    use pumpkin_inventory::player::player_inventory::PlayerInventory;
    use pumpkin_util::GameMode;
    use pumpkin_util::Hand;
    use pumpkin_util::math::position::BlockPos;
    use std::sync::{Arc, Mutex};

    fn inventory() -> PlayerInventory {
        PlayerInventory::new(
            Arc::new(Mutex::new(EntityEquipment::new())),
            Arc::new(build_equipment_slots()),
        )
    }

    #[test]
    fn insert_then_take_keeps_a_single_book() {
        let inventory = inventory();
        inventory.set_held_item(ItemStack::new(1, &Item::BOOK));
        let hand = match Hand::from_packet_id(0) {
            Ok(hand) => hand,
            Err(_) => panic!("packet hand 0 is the main hand"),
        };
        let mut hand_stack = inventory.get_stack_in_hand(hand);
        let before = hand_stack.clone();
        let shelf = ChiseledBookshelfBlockEntity::new(BlockPos::ZERO);
        shelf.set_book(0, hand_stack.split_unless_creative(GameMode::Survival, 1));
        crate::item::store_used_hand(&inventory, hand, &before, hand_stack);

        assert!(inventory.held_item().is_empty());
        assert!(inventory.off_hand_item().is_empty());
        assert_eq!(shelf.get_stack(0).item_count, 1);

        let taken = shelf.remove_book(0, 1);
        assert!(shelf.get_stack(0).is_empty());
        assert_eq!(taken.item_count, 1);
        inventory.set_held_item(taken);
        assert_eq!(inventory.held_item().item_count, 1);
        assert!(inventory.off_hand_item().is_empty());
    }

    #[test]
    fn inserting_one_of_a_stack_leaves_the_rest_in_that_hand() {
        let inventory = inventory();
        inventory.set_held_item(ItemStack::new(3, &Item::BOOK));
        let hand = match Hand::from_packet_id(0) {
            Ok(hand) => hand,
            Err(_) => panic!("packet hand 0 is the main hand"),
        };
        let mut hand_stack = inventory.get_stack_in_hand(hand);
        let before = hand_stack.clone();
        let shelf = ChiseledBookshelfBlockEntity::new(BlockPos::ZERO);
        shelf.set_book(0, hand_stack.split_unless_creative(GameMode::Survival, 1));
        crate::item::store_used_hand(&inventory, hand, &before, hand_stack);

        assert_eq!(inventory.held_item().item_count, 2);
        assert!(inventory.off_hand_item().is_empty());
        assert_eq!(shelf.get_stack(0).item_count, 1);
    }
}
