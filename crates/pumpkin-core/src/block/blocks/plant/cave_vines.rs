use pumpkin_data::block_properties::{CaveVinesLikeProperties, CaveVinesPlantLikeProperties};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockDirection, BlockId, BlockState, BlockStateId};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::{BlockAccessor, BlockFlags};

use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, BlockMetadata, BonemealArgs, CanPlaceAtArgs, GetStateForNeighborUpdateArgs,
    NormalUseArgs, PlacedArgs,
};

pub struct CaveVinesBlock;

impl BlockMetadata for CaveVinesBlock {
    fn ids() -> Box<[BlockId]> {
        [BlockId::CAVE_VINES, BlockId::CAVE_VINES_PLANT].into()
    }
}

impl CaveVinesBlock {
    #[must_use]
    pub fn can_survive(world: &dyn BlockAccessor, pos: &BlockPos) -> bool {
        let support_pos = pos.up();
        let (support_block, support_state) = world.get_block_and_state(&support_pos);
        if support_block == &Block::CAVE_VINES || support_block == &Block::CAVE_VINES_PLANT {
            return true;
        }
        support_state.is_side_solid(BlockDirection::Down) && support_block.is_solid()
    }
}

impl BlockBehaviour for CaveVinesBlock {
    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        Self::can_survive(args.block_accessor, args.position)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if !Self::can_survive(args.world, args.position) {
            return Block::AIR.default_state.id;
        }
        args.state_id
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let support_pos = args.position.up();
        let support_block = args.world.get_block(&support_pos);
        if support_block == &Block::CAVE_VINES {
            let support_state_id = args.world.get_block_state_id(&support_pos);
            let support_props = CaveVinesLikeProperties::from_state_id(support_state_id);
            let mut plant_props = CaveVinesPlantLikeProperties::default(&Block::CAVE_VINES_PLANT);
            plant_props.berries = support_props.berries;
            args.world.set_block_state(
                &support_pos,
                plant_props.to_state_id(&Block::CAVE_VINES_PLANT),
                BlockFlags::empty(),
            );
        }
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        let state_id = args.world.get_block_state_id(args.position);
        if args.block == &Block::CAVE_VINES {
            let mut props = CaveVinesLikeProperties::from_state_id(state_id);
            if props.berries {
                props.berries = false;
                args.world
                    .drop_stack(args.position, ItemStack::new(1, &Item::GLOW_BERRIES));
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_ALL,
                );
                return BlockActionResult::SuccessServer;
            }
        } else if args.block == &Block::CAVE_VINES_PLANT {
            let mut props = CaveVinesPlantLikeProperties::from_state_id(state_id);
            if props.berries {
                props.berries = false;
                args.world
                    .drop_stack(args.position, ItemStack::new(1, &Item::GLOW_BERRIES));
                args.world.set_block_state(
                    args.position,
                    props.to_state_id(args.block),
                    BlockFlags::NOTIFY_ALL,
                );
                return BlockActionResult::SuccessServer;
            }
        }
        BlockActionResult::Pass
    }

    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        if args.block == &Block::CAVE_VINES {
            !CaveVinesLikeProperties::from_state_id(args.state_id).berries
        } else if args.block == &Block::CAVE_VINES_PLANT {
            !CaveVinesPlantLikeProperties::from_state_id(args.state_id).berries
        } else {
            false
        }
    }

    fn adjust_drops(&self, state: &BlockState, mut drops: Vec<ItemStack>) -> Vec<ItemStack> {
        let block = Block::from_state_id(state.id);
        let has_berries = if block == &Block::CAVE_VINES {
            CaveVinesLikeProperties::from_state_id(state.id).berries
        } else if block == &Block::CAVE_VINES_PLANT {
            CaveVinesPlantLikeProperties::from_state_id(state.id).berries
        } else {
            return drops;
        };
        if !has_berries {
            drops.retain(|stack| stack.item.id != Item::GLOW_BERRIES.id);
        }
        drops
    }

    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        if args.block == &Block::CAVE_VINES {
            let mut props = CaveVinesLikeProperties::from_state_id(args.state_id);
            props.berries = true;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL,
            );
        } else if args.block == &Block::CAVE_VINES_PLANT {
            let mut props = CaveVinesPlantLikeProperties::from_state_id(args.state_id);
            props.berries = true;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::BlockState;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;

    fn vine_state(berries: bool) -> &'static BlockState {
        let mut props = CaveVinesLikeProperties::default(&Block::CAVE_VINES);
        props.berries = berries;
        BlockState::from_id(props.to_state_id(&Block::CAVE_VINES))
    }

    fn plant_state(berries: bool) -> &'static BlockState {
        let mut props = CaveVinesPlantLikeProperties::default(&Block::CAVE_VINES_PLANT);
        props.berries = berries;
        BlockState::from_id(props.to_state_id(&Block::CAVE_VINES_PLANT))
    }

    fn glow_berry_count(drops: &[ItemStack]) -> u32 {
        drops
            .iter()
            .filter(|stack| stack.item.id == Item::GLOW_BERRIES.id)
            .map(|stack| u32::from(stack.item_count))
            .sum()
    }

    #[test]
    fn vines_without_berries_do_not_drop_glow_berries() {
        for state in [vine_state(false), plant_state(false)] {
            let drops =
                CaveVinesBlock.adjust_drops(state, vec![ItemStack::new(1, &Item::GLOW_BERRIES)]);
            assert_eq!(glow_berry_count(&drops), 0);
        }
    }

    #[test]
    fn vines_with_berries_drop_one_glow_berry() {
        for state in [vine_state(true), plant_state(true)] {
            let drops =
                CaveVinesBlock.adjust_drops(state, vec![ItemStack::new(1, &Item::GLOW_BERRIES)]);
            assert_eq!(glow_berry_count(&drops), 1);
        }
    }
}
