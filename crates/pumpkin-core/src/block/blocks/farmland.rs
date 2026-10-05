use std::sync::Arc;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnPlaceArgs,
    OnScheduledTickArgs, PathComputationType, RandomTickArgs,
};
use crate::world::World;
use pumpkin_data::block_properties::FarmlandLikeProperties;
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;
use pumpkin_world::world::BlockFlags;

type FarmlandProperties = FarmlandLikeProperties;

#[pumpkin_block("minecraft:farmland")]
pub struct FarmlandBlock;

impl BlockBehaviour for FarmlandBlock {
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        // TODO: push up entities
        args.world.set_block_state(
            args.position,
            Block::DIRT.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            return Block::DIRT.default_state.id;
        }
        args.block.default_state.id
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Up && !can_place_at(args.world, args.position) {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_at(args.block_accessor, args.position)
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        let state_id = args.world.get_block_state_id(args.position);
        let mut props = FarmlandProperties::from_state_id(state_id);
        let above = args.position.up();
        let mut new_moisture = match farmland_tick(
            props.moisture,
            is_water_nearby(args.world, args.position),
            rain_reaches_farmland(
                args.world.is_raining(),
                args.world.get_block_state(&above).is_solid(),
            ),
            args.world
                .get_block(&above)
                .has_tag(&tag::Block::MINECRAFT_MAINTAINS_FARMLAND),
        ) {
            FarmlandTick::Stay => return,
            FarmlandTick::RevertToDirt => {
                //TODO push entities up
                args.world.set_block_state(
                    args.position,
                    Block::DIRT.default_state.id,
                    BlockFlags::NOTIFY_NEIGHBORS,
                );
                return;
            }
            FarmlandTick::Hydrate => 7,
            FarmlandTick::Dry => (props.moisture as i32 - 1).clamp(0, 7),
        };
        if let Some(server) = args.world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::block::moisture_change::MoistureChangeEvent::new(
                    *args.position,
                    args.world.clone(),
                    new_moisture,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
            new_moisture = event.new_moisture;
        }
        props.moisture = new_moisture.clamp(0, 7) as u8;
        args.world.set_block_state(
            args.position,
            props.to_state_id(args.block),
            BlockFlags::NOTIFY_NEIGHBORS,
        );
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FarmlandTick {
    Hydrate,
    Dry,
    RevertToDirt,
    Stay,
}

const fn rain_reaches_farmland(raining: bool, solid_above: bool) -> bool {
    raining && !solid_above
}

const fn farmland_tick(
    moisture: u8,
    water_nearby: bool,
    rain_reaches: bool,
    maintains_farmland: bool,
) -> FarmlandTick {
    if water_nearby || rain_reaches {
        FarmlandTick::Hydrate
    } else if moisture == 0 {
        if maintains_farmland {
            FarmlandTick::Stay
        } else {
            FarmlandTick::RevertToDirt
        }
    } else {
        FarmlandTick::Dry
    }
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    let state = world.get_block_state(&block_pos.up());
    !state.is_solid() // TODO: add fence gate block
}

fn is_water_nearby(world: &Arc<World>, block_pos: &BlockPos) -> bool {
    for dx in -4..=4 {
        for dy in 0..=1 {
            for dz in -4..=4 {
                let check_pos = block_pos.offset(Vector3 {
                    x: dx,
                    y: dy,
                    z: dz,
                });
                //TODO this should use tag water. It does not seem to work rn.
                if world.get_block(&check_pos) == &Block::WATER {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_hydrates_exposed_farmland_and_dry_weather_dries_it() {
        assert!(rain_reaches_farmland(true, false));
        assert!(!rain_reaches_farmland(true, true));
        assert!(!rain_reaches_farmland(false, false));
        assert_eq!(
            farmland_tick(0, false, rain_reaches_farmland(true, false), false),
            FarmlandTick::Hydrate
        );
        assert_eq!(
            farmland_tick(6, false, rain_reaches_farmland(true, false), true),
            FarmlandTick::Hydrate
        );
        assert_eq!(farmland_tick(4, true, false, false), FarmlandTick::Hydrate);
        assert_eq!(
            farmland_tick(4, false, rain_reaches_farmland(true, true), false),
            FarmlandTick::Dry
        );
        assert_eq!(
            farmland_tick(3, false, rain_reaches_farmland(false, false), false),
            FarmlandTick::Dry
        );
        assert_eq!(
            farmland_tick(0, false, false, false),
            FarmlandTick::RevertToDirt
        );
        assert_eq!(farmland_tick(0, false, false, true), FarmlandTick::Stay);
        assert_eq!(
            farmland_tick(0, false, rain_reaches_farmland(true, true), false),
            FarmlandTick::RevertToDirt
        );
    }
}
