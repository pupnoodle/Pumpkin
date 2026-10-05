use std::any::Any;

use crate::block::registry::BlockActionResult;
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use crate::server::Server;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

pub struct BoneMealItem;

impl ItemMetadata for BoneMealItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::BONE_MEAL.id])
    }
}

impl ItemBehaviour for BoneMealItem {
    #[allow(clippy::too_many_lines)]
    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        location: BlockPos,
        _face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        block: &Block,
        server: &Server,
    ) -> BlockActionResult {
        let world = player.world();
        let state_id = world.get_block_state_id(&location);
        let applied = server
            .block_registry
            .bone_meal(block, &world, &location, state_id);
        if applied {
            world.sync_world_event(WorldEvent::ParticlesAndSoundPlantGrowth, location, 15);
        }
        if spend_bone_meal(applied, player.gamemode.load(), item) {
            BlockActionResult::Success
        } else {
            BlockActionResult::Pass
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn spend_bone_meal(applied: bool, gamemode: GameMode, item: &mut ItemStack) -> bool {
    if applied {
        item.decrement_unless_creative(gamemode, 1);
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survival_bone_meal_is_spent_only_when_the_crop_accepts_it() {
        let mut stack = ItemStack::new(4, &Item::BONE_MEAL);
        assert!(!spend_bone_meal(false, GameMode::Survival, &mut stack));
        assert_eq!(stack.item_count, 4);

        assert!(spend_bone_meal(true, GameMode::Survival, &mut stack));
        assert_eq!(stack.item_count, 3);

        let mut creative = ItemStack::new(4, &Item::BONE_MEAL);
        assert!(spend_bone_meal(true, GameMode::Creative, &mut creative));
        assert_eq!(creative.item_count, 4);
    }
}
