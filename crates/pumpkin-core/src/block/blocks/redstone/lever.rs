use std::sync::Arc;

use crate::block::{
    CanPlaceAtArgs, EmitsRedstonePowerArgs, GetRedstonePowerArgs, GetStateForNeighborUpdateArgs,
    OnPlaceArgs, OnStateReplacedArgs, blocks::abstract_wall_mounting::WallMountedBlock,
};
use pumpkin_data::{
    Block, BlockDirection, BlockStateId, HorizontalFacingExt,
    block_properties::{AttachFace, HorizontalFacing, LeverLikeProperties},
    game_event::GameEvent,
    sound::{Sound, SoundCategory},
};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::entity::EntityBase;
use crate::{
    block::{
        registry::BlockActionResult,
        {BlockBehaviour, NormalUseArgs},
    },
    world::World,
};

fn toggle_lever(world: &Arc<World>, block_pos: &BlockPos) {
    let (block, state) = world.get_block_and_state_id(block_pos);

    let mut lever_props = LeverLikeProperties::from_state_id(state);
    lever_props.powered = !lever_props.powered;
    world.set_block_state(
        block_pos,
        lever_props.to_state_id(block),
        BlockFlags::NOTIFY_ALL,
    );

    LeverBlock::update_neighbors(world, block_pos, lever_props);

    play_lever_sound(world, block_pos, lever_props.powered);

    let game_event = if lever_props.powered {
        GameEvent::BlockActivate
    } else {
        GameEvent::BlockDeactivate
    };
    world.emit_game_event(game_event.name(), block_pos.to_centered_f64());
}

fn play_lever_sound(world: &Arc<World>, block_pos: &BlockPos, powered: bool) {
    world.play_sound_fine(
        Sound::BlockLeverClick,
        SoundCategory::Blocks,
        &block_pos.to_centered_f64(),
        0.3,
        if powered { 0.6 } else { 0.5 },
    );
}

#[pumpkin_block("minecraft:lever")]
pub struct LeverBlock;

impl BlockBehaviour for LeverBlock {
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        toggle_lever(args.world, args.position);
        BlockActionResult::Success
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        let props = LeverLikeProperties::from_state_id(args.state.id);
        if props.powered { 15 } else { 0 }
    }

    fn get_strong_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        let props = LeverLikeProperties::from_state_id(args.state.id);
        if props.powered && props.get_direction() == args.direction {
            15
        } else {
            0
        }
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        let lever_props = LeverLikeProperties::from_state_id(args.old_state_id);
        if updates_neighbors_when_replaced(args.moved, lever_props.powered) {
            Self::update_neighbors(args.world, args.position, lever_props);
        }
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = LeverLikeProperties::default(args.block);
        (props.face, props.facing) = attachment(
            args.direction,
            args.player.get_entity().get_horizontal_facing(),
        );

        props.to_state_id(args.block)
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        // Use the provided direction, or fallback to the current state's direction if missing
        let direction = args
            .direction
            .unwrap_or_else(|| self.get_direction(args.state.id, args.block));

        WallMountedBlock::can_place_at(self, args.block_accessor, args.position, direction)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        WallMountedBlock::get_state_for_neighbor_update(self, args)
    }
}

impl WallMountedBlock for LeverBlock {
    fn get_direction(&self, state_id: BlockStateId, _block: &Block) -> BlockDirection {
        let props = LeverLikeProperties::from_state_id(state_id);
        match props.face {
            AttachFace::Floor => BlockDirection::Up,
            AttachFace::Ceiling => BlockDirection::Down,
            AttachFace::Wall => props.facing.to_block_direction(),
        }
    }
}

impl LeverBlock {
    fn update_neighbors(
        world: &Arc<World>,
        block_pos: &BlockPos,
        lever_props: LeverLikeProperties,
    ) {
        let direction = lever_props.get_direction().opposite();
        world.update_neighbors(block_pos, None);
        world.update_neighbors(&block_pos.offset(direction.to_offset()), None);
    }
}

pub trait LeverLikePropertiesExt {
    fn get_direction(&self) -> BlockDirection;
}

impl LeverLikePropertiesExt for LeverLikeProperties {
    fn get_direction(&self) -> BlockDirection {
        match self.face {
            AttachFace::Ceiling => BlockDirection::Down,
            AttachFace::Floor => BlockDirection::Up,
            AttachFace::Wall => self.facing.to_block_direction(),
        }
    }
}

fn attachment(
    direction: BlockDirection,
    player_facing: HorizontalFacing,
) -> (AttachFace, HorizontalFacing) {
    let face = match direction {
        BlockDirection::Up => AttachFace::Ceiling,
        BlockDirection::Down => AttachFace::Floor,
        _ => AttachFace::Wall,
    };
    let facing = if direction == BlockDirection::Up || direction == BlockDirection::Down {
        player_facing
    } else {
        direction.opposite().to_cardinal_direction()
    };
    (face, facing)
}

const fn updates_neighbors_when_replaced(moved: bool, powered: bool) -> bool {
    !moved && powered
}

#[cfg(test)]
mod tests {
    use pumpkin_data::{
        BlockDirection, HorizontalFacingExt,
        block_properties::{AttachFace, HorizontalFacing},
    };

    use super::{attachment, updates_neighbors_when_replaced};

    #[test]
    fn attaches_to_the_clicked_face() {
        for clicked in [
            BlockDirection::North,
            BlockDirection::South,
            BlockDirection::East,
            BlockDirection::West,
        ] {
            let (face, facing) = attachment(clicked.opposite(), HorizontalFacing::North);
            assert_eq!(face, AttachFace::Wall);
            assert_eq!(facing.to_block_direction(), clicked);
        }

        let (floor, floor_facing) = attachment(BlockDirection::Down, HorizontalFacing::West);
        assert_eq!(floor, AttachFace::Floor);
        assert_eq!(floor_facing, HorizontalFacing::West);

        let (ceiling, ceiling_facing) = attachment(BlockDirection::Up, HorizontalFacing::East);
        assert_eq!(ceiling, AttachFace::Ceiling);
        assert_eq!(ceiling_facing, HorizontalFacing::East);
    }

    #[test]
    fn piston_move_does_not_cut_power() {
        assert!(!updates_neighbors_when_replaced(true, true));
        assert!(!updates_neighbors_when_replaced(true, false));
        assert!(updates_neighbors_when_replaced(false, true));
        assert!(!updates_neighbors_when_replaced(false, false));
    }
}
