use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use super::painting::{hanging_space_occupied, hanging_surface_supports};
use crate::entity::player::Player;
use crate::entity::{Entity, EntityBase, living::LivingEntity};
use crate::server::Server;
use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockDirection;
use pumpkin_data::block_state::BlockState;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
use pumpkin_protocol::java::client::play::{CSetEntityMetadata, Metadata};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

/// An item frame or glow item frame.
///
/// Holds the displayed item and its rotation so that comparators can read the
/// frame's analog output and so frames from vanilla worlds keep their data
/// across save cycles.
pub struct ItemFrameEntity {
    entity: Entity,
    item_stack: Mutex<ItemStack>,
    /// Rotation of the displayed item, always in `0..8`.
    rotation: AtomicU8,
    /// The direction the frame faces, i.e. the axis pointing away from the
    /// block it hangs on. Stored as the vanilla 3D direction index
    /// (0 = down, 1 = up, 2 = north, 3 = south, 4 = west, 5 = east).
    facing: AtomicU8,
    item_drop_chance: AtomicCell<f32>,
    invisible: AtomicBool,
    fixed: AtomicBool,
}

impl ItemFrameEntity {
    /// Facing used when a frame is created without NBT, matching vanilla.
    const DEFAULT_FACING: BlockDirection = BlockDirection::South;

    pub fn new(entity: Entity) -> Self {
        let facing = Self::DEFAULT_FACING.to_index();
        // The spawn packet reads the direction from the entity data field, so
        // it has to agree with `facing` or the frame spawns facing elsewhere.
        entity.data.store(i32::from(facing), Ordering::Relaxed);
        Self {
            entity,
            item_stack: Mutex::new(ItemStack::EMPTY.clone()),
            rotation: AtomicU8::new(0),
            facing: AtomicU8::new(facing),
            item_drop_chance: AtomicCell::new(1.0),
            invisible: AtomicBool::new(false),
            fixed: AtomicBool::new(false),
        }
    }

    pub const fn is_glow(&self) -> bool {
        self.entity.entity_type.id == EntityType::GLOW_ITEM_FRAME.id
    }

    pub const fn get_add_item_sound(&self) -> Sound {
        if self.is_glow() {
            Sound::EntityGlowItemFrameAddItem
        } else {
            Sound::EntityItemFrameAddItem
        }
    }

    pub const fn get_remove_item_sound(&self) -> Sound {
        if self.is_glow() {
            Sound::EntityGlowItemFrameRemoveItem
        } else {
            Sound::EntityItemFrameRemoveItem
        }
    }

    pub const fn get_rotate_item_sound(&self) -> Sound {
        if self.is_glow() {
            Sound::EntityGlowItemFrameRotateItem
        } else {
            Sound::EntityItemFrameRotateItem
        }
    }

    pub const fn get_break_sound(&self) -> Sound {
        if self.is_glow() {
            Sound::EntityGlowItemFrameBreak
        } else {
            Sound::EntityItemFrameBreak
        }
    }

    pub const fn get_place_sound(&self) -> Sound {
        if self.is_glow() {
            Sound::EntityGlowItemFramePlace
        } else {
            Sound::EntityItemFramePlace
        }
    }

    #[must_use]
    pub fn calculate_pos(location: BlockPos, face: BlockDirection) -> Vector3<f64> {
        let target = location.offset(face.to_offset());
        let step = face.to_offset();
        Vector3::new(
            f64::from(target.0.x) + 0.5 - f64::from(step.x) * 0.46875,
            f64::from(target.0.y) + 0.5 - f64::from(step.y) * 0.46875,
            f64::from(target.0.z) + 0.5 - f64::from(step.z) * 0.46875,
        )
    }

    #[must_use]
    pub fn blocks_fit(
        location: BlockPos,
        face: BlockDirection,
        state_at: impl Fn(BlockPos) -> &'static BlockState,
    ) -> bool {
        let front = location.offset(face.to_offset());
        hanging_surface_supports(face, state_at(location), state_at(front))
    }

    #[must_use]
    pub fn can_place(
        world: &crate::world::World,
        location: BlockPos,
        face: BlockDirection,
    ) -> bool {
        if !Self::blocks_fit(location, face, |pos| world.get_block_state(&pos)) {
            return false;
        }
        let front = location.offset(face.to_offset());
        !hanging_space_occupied(world, &[front], face)
    }

    pub fn get_facing(&self) -> BlockDirection {
        BlockDirection::from_index(self.facing.load(Ordering::Relaxed))
            .unwrap_or(Self::DEFAULT_FACING)
    }

    pub fn set_facing(&self, facing: BlockDirection) {
        let index = facing.to_index();
        self.facing.store(index, Ordering::Relaxed);
        self.entity.data.store(i32::from(index), Ordering::Relaxed);
    }

    pub fn get_item(&self) -> ItemStack {
        self.item_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn set_item(&self, mut item_stack: ItemStack, update_neighbours: bool) {
        if !item_stack.is_empty() {
            item_stack.item_count = 1;
        }

        let play_sound = !item_stack.is_empty();
        let item_serializer = ItemStackSerializer::from(item_stack.clone());
        *self
            .item_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = item_stack;

        self.entity.set_synced_data(
            pumpkin_data::tracked_data::item_frame::ITEM,
            item_serializer,
        );

        if play_sound {
            self.entity.play_sound(self.get_add_item_sound());
        }

        if update_neighbours {
            let world = self.entity.world.load();
            let pos = self.entity.block_pos.load();
            world.update_neighbors(&pos, None);
        }
    }

    pub fn get_rotation(&self) -> u8 {
        self.rotation.load(Ordering::Relaxed) % 8
    }

    pub fn set_rotation(&self, rotation: u8, update_neighbours: bool) {
        let rot = rotation % 8;
        self.rotation.store(rot, Ordering::Relaxed);

        self.entity
            .set_synced_data(pumpkin_data::tracked_data::item_frame::ROTATION, rot as i32);

        if update_neighbours {
            let world = self.entity.world.load();
            let pos = self.entity.block_pos.load();
            world.update_neighbors(&pos, None);
        }
    }

    pub fn get_drop_chance(&self) -> f32 {
        self.item_drop_chance.load()
    }

    pub fn set_drop_chance(&self, chance: f32) {
        self.item_drop_chance.store(chance);
    }

    pub fn is_fixed(&self) -> bool {
        self.fixed.load(Ordering::Relaxed)
    }

    pub fn set_fixed(&self, fixed: bool) {
        self.fixed.store(fixed, Ordering::Relaxed);
    }

    pub fn is_invisible(&self) -> bool {
        self.invisible.load(Ordering::Relaxed)
    }

    pub fn set_invisible(&self, invisible: bool) {
        self.invisible.store(invisible, Ordering::Relaxed);
    }

    pub fn get_frame_item_stack(&self) -> ItemStack {
        if self.is_glow() {
            ItemStack::new(1, &Item::GLOW_ITEM_FRAME)
        } else {
            ItemStack::new(1, &Item::ITEM_FRAME)
        }
    }

    pub fn get_frame_item_stack_with_data(&self) -> ItemStack {
        let mut stack = self.get_frame_item_stack();
        if let Some(custom_name) = self.entity.custom_name.load().as_ref().clone() {
            stack.set_custom_name(custom_name.to_pretty_console());
        }
        stack
    }

    pub fn get_pick_result(&self) -> ItemStack {
        let framed_stack = self.get_item();
        if framed_stack.is_empty() {
            self.get_frame_item_stack_with_data()
        } else {
            framed_stack
        }
    }

    /// The comparator signal this frame produces.
    ///
    /// Vanilla: `getItem().isEmpty() ? 0 : getRotation() % 8 + 1`.
    pub fn get_analog_output(&self) -> u8 {
        if self
            .item_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
        {
            0
        } else {
            self.rotation.load(Ordering::Relaxed) % 8 + 1
        }
    }

    pub fn drop_item(&self, caused_by: Option<&dyn EntityBase>, with_frame: bool) {
        if self.is_fixed() {
            return;
        }

        let item_stack = self.get_item();
        *self
            .item_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ItemStack::EMPTY.clone();
        let item_serializer = ItemStackSerializer::from(ItemStack::EMPTY.clone());
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::item_frame::ITEM,
            item_serializer,
        );

        let world = self.entity.world.load();
        let creative = caused_by.is_some_and(|cause| {
            cause
                .cast_any()
                .downcast_ref::<Player>()
                .is_some_and(Player::is_creative)
        });
        let (drop_frame, drop_contents) = frame_break_drops(
            world.level_info.load().game_rules.entity_drops,
            creative,
            if with_frame {
                FrameHit::Break
            } else {
                FrameHit::PopItem
            },
            !item_stack.is_empty(),
        );
        let pos = self.entity.block_pos.load();
        if drop_frame {
            world.drop_stack(&pos, self.get_frame_item_stack_with_data());
        }
        if drop_contents && rand::random::<f32>() < self.item_drop_chance.load() {
            world.drop_stack(&pos, item_stack);
        }
    }

    fn pop_off(&self) {
        if self.entity.removed.load(Ordering::Relaxed) || self.is_fixed() {
            return;
        }
        let world = self.entity.world.load();
        let entity_drops = world.level_info.load().game_rules.entity_drops;
        self.drop_item(None, true);
        if entity_drops {
            self.entity.play_sound(self.get_break_sound());
        }
        self.entity.remove();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameHit {
    PopItem,
    Break,
}

#[must_use]
pub const fn frame_break_drops(
    entity_drops: bool,
    creative: bool,
    hit: FrameHit,
    has_contents: bool,
) -> (bool, bool) {
    let allow = entity_drops && !creative;
    (
        allow && matches!(hit, FrameHit::Break),
        allow && has_contents,
    )
}

impl EntityBase for ItemFrameEntity {
    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        let item = self
            .item_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !item.is_empty() {
            let mut item_compound = NbtCompound::new();
            item.write_item_stack(&mut item_compound);
            nbt.put_compound("Item", item_compound);
        }
        nbt.put_float("ItemDropChance", self.item_drop_chance.load());
        nbt.put_byte("ItemRotation", self.rotation.load(Ordering::Relaxed) as i8);
        nbt.put_byte("Facing", self.facing.load(Ordering::Relaxed) as i8);
        nbt.put_bool("Invisible", self.invisible.load(Ordering::Relaxed));
        nbt.put_bool("Fixed", self.fixed.load(Ordering::Relaxed));
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        if let Some(item_compound) = nbt.get_compound("Item")
            && let Some(stack) = ItemStack::read_item_stack(item_compound)
        {
            *self
                .item_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = stack;
        }
        self.rotation.store(
            (nbt.get_byte("ItemRotation").unwrap_or(0) as u8) % 8,
            Ordering::Relaxed,
        );
        let facing = nbt.get_byte("Facing").unwrap_or(0) as u8 % 6;
        self.facing.store(facing, Ordering::Relaxed);
        // The spawn packet's data field carries the frame's direction.
        self.entity.data.store(i32::from(facing), Ordering::Relaxed);
        self.item_drop_chance
            .store(nbt.get_float("ItemDropChance").unwrap_or(1.0));
        self.invisible.store(
            nbt.get_bool("Invisible").unwrap_or(false),
            Ordering::Relaxed,
        );
        self.fixed
            .store(nbt.get_bool("Fixed").unwrap_or(false), Ordering::Relaxed);
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn tick(&self, caller: &dyn EntityBase, server: &Server) {
        self.entity.tick(caller, server);
        if self.entity.removed.load(Ordering::Relaxed) || self.is_fixed() {
            return;
        }
        let world = self.entity.world.load();
        let front = self.entity.block_pos.load();
        let face = self.get_facing();
        let support = front.offset(face.opposite().to_offset());
        let Some(wall) = world.get_block_state_if_loaded(&support) else {
            return;
        };
        let Some(front_state) = world.get_block_state_if_loaded(&front) else {
            return;
        };
        if !hanging_surface_supports(face, wall, front_state) {
            self.pop_off();
        }
    }

    fn init_data_tracker(&self) {
        let item_serializer = ItemStackSerializer::from(
            self.item_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        );
        let rotation = self.get_rotation() as i32;

        self.entity.set_synced_data(
            pumpkin_data::tracked_data::item_frame::ITEM,
            item_serializer,
        );
        self.entity
            .set_synced_data(pumpkin_data::tracked_data::item_frame::ROTATION, rotation);
    }

    fn send_java_spawn_packet(&self, client: &crate::net::java::JavaClient) {
        let spawn_packet = self.entity.create_spawn_packet();
        if let Ok(data) = client.serialize_packet(&spawn_packet) {
            client.try_enqueue_packet(data);
        }

        let ver = pumpkin_data::packet::CURRENT_MC_VERSION;
        let item_serializer = ItemStackSerializer::from(
            self.item_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        );
        let rotation = self.get_rotation() as i32;

        let mut data = Vec::new();
        let meta_item = Metadata::new(
            pumpkin_data::tracked_data::item_frame::ITEM,
            item_serializer,
        );
        let meta_rot = Metadata::new(pumpkin_data::tracked_data::item_frame::ROTATION, rotation);

        if meta_item.write(&mut data, &ver).is_ok() && meta_rot.write(&mut data, &ver).is_ok() {
            data.push(255);
            let meta_packet = CSetEntityMetadata::new(self.entity.entity_id.into(), data.into());
            if let Ok(meta_data) = client.serialize_packet(&meta_packet) {
                client.try_enqueue_packet(meta_data);
            }
        }
    }

    fn interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.is_fixed() {
            return false;
        }

        let frame_has_item = !self.get_item().is_empty();
        let has_held_item = !item_stack.is_empty();

        if frame_has_item {
            let new_rot = self.get_rotation() + 1;
            self.set_rotation(new_rot, true);
            self.entity.play_sound(self.get_rotate_item_sound());
            true
        } else if has_held_item && !self.entity.removed.load(Ordering::Relaxed) {
            let mut new_stack = item_stack.clone();
            new_stack.item_count = 1;
            self.set_item(new_stack, true);

            if !player.is_creative() {
                item_stack.decrement(1);
            }
            true
        } else {
            false
        }
    }

    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        _amount: f32,
        damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        let attacker = source.or(cause);
        let is_creative_player = attacker.is_some_and(|cause| {
            cause
                .cast_any()
                .downcast_ref::<Player>()
                .is_some_and(Player::is_creative)
        });
        let bypasses_invuln =
            damage_type == DamageType::OUT_OF_WORLD || damage_type == DamageType::GENERIC_KILL;

        if self.is_fixed() && !bypasses_invuln && !is_creative_player {
            return false;
        }

        let has_item = !self.get_item().is_empty();
        let is_explosion =
            damage_type == DamageType::EXPLOSION || damage_type == DamageType::PLAYER_EXPLOSION;
        let break_frame = self.is_fixed() || is_creative_player || is_explosion || !has_item;

        self.drop_item(attacker, break_frame);
        if break_frame {
            self.entity.play_sound(self.get_break_sound());
            self.entity.remove();
        } else {
            self.entity.play_sound(self.get_remove_item_sound());
        }
        true
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameHit, ItemFrameEntity, frame_break_drops};
    use pumpkin_data::BlockDirection;
    use pumpkin_data::BlockState;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector3::Vector3;

    fn walls(
        positions: Vec<BlockPos>,
        wall: &'static BlockState,
    ) -> impl Fn(BlockPos) -> &'static BlockState {
        let air = pumpkin_data::Block::AIR.default_state;
        move |pos| {
            if positions.contains(&pos) { wall } else { air }
        }
    }

    #[test]
    fn stone_face_with_air_accepts_a_frame_on_every_side() {
        let location = BlockPos(Vector3::new(4, 64, 8));
        let at = walls(vec![location], pumpkin_data::Block::STONE.default_state);
        for face in BlockDirection::all() {
            assert!(ItemFrameEntity::blocks_fit(location, face, &at), "{face:?}");
        }
    }

    #[test]
    fn frame_rejects_air_a_blocked_front_and_unsturdy_faces() {
        let location = BlockPos(Vector3::new(4, 64, 8));
        let air_only = walls(Vec::new(), pumpkin_data::Block::STONE.default_state);
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::North,
            &air_only
        ));

        let stone = pumpkin_data::Block::STONE.default_state;
        let blocked = |pos: BlockPos| {
            if pos == location || pos == location.offset(BlockDirection::Up.to_offset()) {
                stone
            } else {
                pumpkin_data::Block::AIR.default_state
            }
        };
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::Up,
            blocked
        ));

        let slab = walls(vec![location], pumpkin_data::Block::OAK_SLAB.default_state);
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::Up,
            &slab
        ));
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::North,
            &slab
        ));
        assert!(ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::Down,
            &slab
        ));

        let snow = walls(vec![location], pumpkin_data::Block::SNOW.default_state);
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::Up,
            &snow
        ));
        assert!(!ItemFrameEntity::blocks_fit(
            location,
            BlockDirection::South,
            &snow
        ));
    }

    #[test]
    fn frame_sits_against_the_supporting_face() {
        let location = BlockPos(Vector3::new(10, 64, 20));
        let north = ItemFrameEntity::calculate_pos(location, BlockDirection::North);
        assert_eq!(north.x, 10.5);
        assert_eq!(north.y, 64.5);
        assert!((north.z - 19.96875).abs() < 1e-6);

        let up = ItemFrameEntity::calculate_pos(location, BlockDirection::Up);
        assert_eq!(up.x, 10.5);
        assert!((up.y - 65.03125).abs() < 1e-6);
        assert_eq!(up.z, 20.5);
    }

    #[test]
    fn survival_break_drops_the_frame_and_contents_separately() {
        assert_eq!(
            frame_break_drops(true, false, FrameHit::Break, false),
            (true, false)
        );
        assert_eq!(
            frame_break_drops(true, false, FrameHit::PopItem, true),
            (false, true)
        );
        assert_eq!(
            frame_break_drops(true, false, FrameHit::Break, true),
            (true, true)
        );
        assert_eq!(
            frame_break_drops(true, true, FrameHit::Break, true),
            (false, false)
        );
        assert_eq!(
            frame_break_drops(false, false, FrameHit::Break, true),
            (false, false)
        );
    }
}
