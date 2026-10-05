use core::f32;
use std::sync::atomic::{AtomicU32, Ordering};

use super::item_frame::ItemFrameEntity;
use crate::entity::player::Player;
use crate::entity::{Entity, EntityBase, living::LivingEntity};
use pumpkin_data::block_state::BlockState;
use pumpkin_data::damage::DamageType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::painting_variant::PaintingVariant;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, BlockDirection};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::Metadata;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::version::JavaMinecraftVersion;

/// The world stores a painting facing as a horizontal value: 0 south, 1 west,
/// 2 north, 3 east (`Direction.get2DDataValue`). The entity data, and the spawn
/// packet built from it, use the 3D index instead: north 2, south 3, west 4,
/// east 5. Sending the raw horizontal value makes the client read 0 as down,
/// fail the horizontal check in `HangingEntity.setDirection` and disconnect.
const fn facing_from_horizontal(value: u8) -> BlockDirection {
    match value & 3 {
        1 => BlockDirection::West,
        2 => BlockDirection::North,
        3 => BlockDirection::East,
        _ => BlockDirection::South,
    }
}

/// Vanilla reads the field with `getByte`, which yields 0 (south) when it is
/// absent, so a partial entity file keeps a horizontal default.
fn facing_from_nbt(nbt: &NbtCompound) -> BlockDirection {
    facing_from_horizontal(nbt.get_byte("facing").unwrap_or(0) as u8)
}

const fn facing_to_horizontal(direction: BlockDirection) -> u8 {
    match direction {
        BlockDirection::West => 1,
        BlockDirection::North => 2,
        BlockDirection::East => 3,
        _ => 0,
    }
}

pub struct PaintingEntity {
    pub entity: Entity,
    variant_id: AtomicU32,
}

impl PaintingEntity {
    #[must_use]
    pub const fn new(entity: Entity) -> Self {
        Self::new_with_variant(entity, PaintingVariant::Alban)
    }

    #[must_use]
    pub const fn new_with_variant(entity: Entity, variant: PaintingVariant) -> Self {
        Self {
            entity,
            variant_id: AtomicU32::new(variant.id()),
        }
    }

    #[must_use]
    pub fn variant(&self) -> PaintingVariant {
        let id = self.variant_id.load(Ordering::Relaxed);
        PaintingVariant::all()
            .get(id as usize)
            .copied()
            .unwrap_or(PaintingVariant::Alban)
    }

    pub fn set_variant(&self, variant: PaintingVariant) {
        self.variant_id.store(variant.id(), Ordering::Relaxed);
        self.sync_variant();
    }

    pub fn sync_variant(&self) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::painting::DATA_PAINTING_VARIANT_ID,
            VarInt(self.variant().id() as i32),
        );
    }

    /// Calculates the exact floating-point center position for a painting of given width and height (in blocks),
    /// placed on `face` of `location`.
    #[must_use]
    pub fn calculate_center_pos(
        location: BlockPos,
        face: BlockDirection,
        width: u32,
        height: u32,
    ) -> Vector3<f64> {
        let target_pos = location.offset(face.to_offset());
        let face_ccw = face.rotate_counter_clockwise();
        let step_x = f64::from(face.to_offset().x);
        let step_z = f64::from(face.to_offset().z);
        let ccw_step_x = f64::from(face_ccw.to_offset().x);
        let ccw_step_z = f64::from(face_ccw.to_offset().z);

        let mut x = f64::from(target_pos.0.x) + 0.5 - step_x * 0.46875;
        let mut y = f64::from(target_pos.0.y) + 0.5;
        let mut z = f64::from(target_pos.0.z) + 0.5 - step_z * 0.46875;

        let width_offset = if width.is_multiple_of(2) { 0.5 } else { 0.0 };
        let height_offset = if height.is_multiple_of(2) { 0.5 } else { 0.0 };

        x += width_offset * ccw_step_x;
        z += width_offset * ccw_step_z;
        y += height_offset;

        Vector3::new(x, y, z)
    }

    /// A painting fits when every block it covers has a sturdy face (or a repeater or
    /// comparator, on a side) and air in front, and nothing else already hangs there.
    #[must_use]
    pub fn painting_fits(
        world: &crate::world::World,
        location: BlockPos,
        face: BlockDirection,
        variant: PaintingVariant,
    ) -> bool {
        if !Self::painting_blocks_fit(location, face, variant.width(), variant.height(), |pos| {
            world.get_block_state(&pos)
        }) {
            return false;
        }
        let fronts = Self::front_blocks(location, face, variant.width(), variant.height());
        !hanging_space_occupied(world, &fronts, face)
    }

    #[must_use]
    pub fn painting_blocks_fit(
        location: BlockPos,
        face: BlockDirection,
        width: u32,
        height: u32,
        state_at: impl Fn(BlockPos) -> &'static BlockState,
    ) -> bool {
        if !face.is_horizontal() || width == 0 || height == 0 {
            return false;
        }
        for (wall_pos, front_pos) in Self::covered_cells(location, face, width, height) {
            if !hanging_surface_supports(face, state_at(wall_pos), state_at(front_pos)) {
                return false;
            }
        }
        true
    }

    fn covered_cells(
        location: BlockPos,
        face: BlockDirection,
        width: u32,
        height: u32,
    ) -> Vec<(BlockPos, BlockPos)> {
        let face_ccw = face.rotate_counter_clockwise();
        let ccw_offset = face_ccw.to_offset();
        let face_offset = face.to_offset();
        let k = -((width as i32 - 1) / 2);
        let l = -((height as i32 - 1) / 2);
        let mut cells = Vec::with_capacity((width * height) as usize);
        for x_idx in 0..width {
            for y_idx in 0..height {
                let wall_pos = BlockPos(Vector3::new(
                    location.0.x + ccw_offset.x * (x_idx as i32 + k),
                    location.0.y + (y_idx as i32 + l),
                    location.0.z + ccw_offset.z * (x_idx as i32 + k),
                ));
                let front_pos = wall_pos.offset(face_offset);
                cells.push((wall_pos, front_pos));
            }
        }
        cells
    }

    fn front_blocks(
        location: BlockPos,
        face: BlockDirection,
        width: u32,
        height: u32,
    ) -> Vec<BlockPos> {
        Self::covered_cells(location, face, width, height)
            .into_iter()
            .map(|(_, front)| front)
            .collect()
    }

    #[must_use]
    pub fn anchor_from_center(
        center: Vector3<f64>,
        face: BlockDirection,
        width: u32,
        height: u32,
    ) -> Option<BlockPos> {
        if !face.is_horizontal() {
            return None;
        }
        let step = face.to_offset();
        let ccw = face.rotate_counter_clockwise().to_offset();
        let width_offset = if width.is_multiple_of(2) { 0.5 } else { 0.0 };
        let height_offset = if height.is_multiple_of(2) { 0.5 } else { 0.0 };
        let target_x = (center.x - 0.5 + f64::from(step.x) * 0.46875
            - width_offset * f64::from(ccw.x))
        .round() as i32;
        let target_y = (center.y - 0.5 - height_offset).round() as i32;
        let target_z = (center.z - 0.5 + f64::from(step.z) * 0.46875
            - width_offset * f64::from(ccw.z))
        .round() as i32;
        Some(
            BlockPos(Vector3::new(target_x, target_y, target_z))
                .offset(face.opposite().to_offset()),
        )
    }

    /// Chooses a placeable painting variant that fits the wall at `location` facing `face`.
    /// Matches vanilla Minecraft: filters to variants that fit, selects the subset with
    /// the largest area (width * height), and randomly picks one from that subset.
    #[must_use]
    pub fn choose_variant(
        world: &crate::world::World,
        location: BlockPos,
        face: BlockDirection,
    ) -> Option<PaintingVariant> {
        use rand::seq::IndexedRandom;

        let mut fitting_variants = Vec::new();
        let mut max_area = 0;

        for &variant in PaintingVariant::all_placeable() {
            if Self::painting_fits(world, location, face, variant) {
                let area = variant.width() * variant.height();
                if area > max_area {
                    max_area = area;
                    fitting_variants.clear();
                    fitting_variants.push(variant);
                } else if area == max_area {
                    fitting_variants.push(variant);
                }
            }
        }

        let mut rng = rand::rng();
        fitting_variants.choose(&mut rng).copied()
    }

    fn drop_and_remove(&self, caused_by: Option<&dyn EntityBase>) {
        let entity = &self.entity;
        let world = entity.world.load();
        let entity_drops = world.level_info.load().game_rules.entity_drops;
        let creative = attacker_is_creative(caused_by);
        if entity_drops {
            world.play_sound(
                Sound::EntityPaintingBreak,
                SoundCategory::Blocks,
                &entity.pos.load(),
            );
        }
        if painting_should_drop(entity_drops, creative) {
            world.drop_stack(&entity.block_pos.load(), ItemStack::new(1, &Item::PAINTING));
        }
        entity.remove();
    }
}

#[must_use]
pub const fn painting_should_drop(entity_drops: bool, creative_attacker: bool) -> bool {
    entity_drops && !creative_attacker
}

#[must_use]
pub fn hanging_surface_supports(
    face: BlockDirection,
    wall: &BlockState,
    front: &BlockState,
) -> bool {
    if !front.is_air() {
        return false;
    }
    if wall.is_side_solid(face) {
        return true;
    }
    face.is_horizontal() && is_redstone_diode(wall)
}

fn is_redstone_diode(state: &BlockState) -> bool {
    let block = Block::from_state_id(state.id);
    block == &Block::REPEATER || block == &Block::COMPARATOR
}

fn attacker_is_creative(caused_by: Option<&dyn EntityBase>) -> bool {
    caused_by.is_some_and(|cause| {
        cause
            .cast_any()
            .downcast_ref::<Player>()
            .is_some_and(Player::is_creative)
    })
}

pub(crate) fn hanging_space_occupied(
    world: &crate::world::World,
    fronts: &[BlockPos],
    face: BlockDirection,
) -> bool {
    let Some(first) = fronts.first() else {
        return false;
    };
    let mut min_x = first.0.x;
    let mut min_y = first.0.y;
    let mut min_z = first.0.z;
    let mut max_x = first.0.x;
    let mut max_y = first.0.y;
    let mut max_z = first.0.z;
    for pos in fronts.iter().skip(1) {
        min_x = min_x.min(pos.0.x);
        min_y = min_y.min(pos.0.y);
        min_z = min_z.min(pos.0.z);
        max_x = max_x.max(pos.0.x);
        max_y = max_y.max(pos.0.y);
        max_z = max_z.max(pos.0.z);
    }
    let bounds = BoundingBox {
        min: Vector3::new(f64::from(min_x), f64::from(min_y), f64::from(min_z)),
        max: Vector3::new(
            f64::from(max_x) + 1.0,
            f64::from(max_y) + 1.0,
            f64::from(max_z) + 1.0,
        ),
    }
    .expand(4.0, 4.0, 4.0);

    for entity in world.get_entities_at_box(&bounds) {
        if entity.get_entity().removed.load(Ordering::Relaxed) {
            continue;
        }
        if let Some(frame) = entity.cast_any().downcast_ref::<ItemFrameEntity>() {
            if frame.get_facing() == face && fronts.contains(&frame.get_entity().block_pos.load()) {
                return true;
            }
            continue;
        }
        let Some(painting) = entity.cast_any().downcast_ref::<PaintingEntity>() else {
            continue;
        };
        let Some(facing) =
            BlockDirection::from_index(painting.get_entity().data.load(Ordering::Relaxed) as u8)
        else {
            continue;
        };
        if facing != face {
            continue;
        }
        let variant = painting.variant();
        let Some(anchor) = PaintingEntity::anchor_from_center(
            painting.get_entity().pos.load(),
            facing,
            variant.width(),
            variant.height(),
        ) else {
            continue;
        };
        let occupied =
            PaintingEntity::front_blocks(anchor, facing, variant.width(), variant.height());
        if occupied.iter().any(|pos| fronts.contains(pos)) {
            return true;
        }
    }
    false
}

impl EntityBase for PaintingEntity {
    fn write_custom_nbt(&self, nbt: &mut NbtCompound) {
        let index = self.entity.data.load(Ordering::Relaxed) as u8;
        let direction = BlockDirection::from_index(index).unwrap_or(BlockDirection::South);
        nbt.put_byte("facing", facing_to_horizontal(direction) as i8);
        nbt.put_string("variant", self.variant().asset_id().to_string());
    }

    fn read_custom_nbt(&self, nbt: &NbtCompound) {
        let facing = facing_from_nbt(nbt);
        self.entity
            .data
            .store(i32::from(facing.to_index()), Ordering::Relaxed);
        if let Some(variant_str) = nbt
            .get_string("variant")
            .or_else(|| nbt.get_string("Motive"))
            && let Some(variant) = PaintingVariant::from_name(variant_str)
        {
            self.set_variant(variant);
        }
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn init_data_tracker(&self) {
        self.sync_variant();
    }

    fn java_spawn_metadata(&self, version: JavaMinecraftVersion) -> Option<Box<[u8]>> {
        let mut metadata = Vec::new();
        Metadata::new(
            pumpkin_data::tracked_data::painting::DATA_PAINTING_VARIANT_ID,
            VarInt(self.variant().id() as i32),
        )
        .write(&mut metadata, &version)
        .ok()?;
        metadata.push(255);
        Some(metadata.into_boxed_slice())
    }

    fn set_variant_name(&self, name: &str) {
        if let Some(variant) = PaintingVariant::from_name(name) {
            self.set_variant(variant);
        }
    }

    fn damage_with_context(
        &self,
        _caller: &dyn EntityBase,
        _amount: f32,
        _damage_type: DamageType,
        _position: Option<Vector3<f64>>,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) -> bool {
        self.drop_and_remove(source.or(cause));
        true
    }

    fn can_hit(&self) -> bool {
        self.entity.is_alive()
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_and_3d_facings_round_trip() {
        for horizontal in 0u8..4 {
            let direction = facing_from_horizontal(horizontal);
            assert!(direction.is_horizontal(), "{direction:?} is vertical");
            assert_eq!(facing_to_horizontal(direction), horizontal);
        }
    }

    #[test]
    fn south_two_is_not_down_zero() {
        // The old bug: south (0 in the world file) was sent as index 0, which the
        // client reads as down and rejects.
        assert_eq!(facing_from_horizontal(0), BlockDirection::South);
        assert_eq!(facing_from_horizontal(0).to_index(), 3);
    }

    #[test]
    fn missing_facing_defaults_to_south() {
        let nbt = NbtCompound::new();
        assert_eq!(facing_from_nbt(&nbt), BlockDirection::South);
        assert_eq!(facing_from_nbt(&nbt).to_index(), 3);

        let mut with_facing = NbtCompound::new();
        with_facing.put_byte("facing", 1);
        assert_eq!(facing_from_nbt(&with_facing), BlockDirection::West);
    }

    #[test]
    fn out_of_range_values_wrap_into_horizontal_directions() {
        assert!(facing_from_horizontal(4).is_horizontal());
        assert!(facing_from_horizontal(255).is_horizontal());
    }

    #[test]
    fn calculate_center_pos_works() {
        let location = BlockPos(Vector3::new(10, 64, 20));

        // 1x1 painting facing North: placed against block at (10, 64, 20)
        let pos_1x1 = PaintingEntity::calculate_center_pos(location, BlockDirection::North, 1, 1);
        assert_eq!(pos_1x1.x, 10.5);
        assert_eq!(pos_1x1.y, 64.5);
        assert!((pos_1x1.z - 19.96875).abs() < 1e-6);

        // 2x1 painting facing North: width is 2, shifts along counter-clockwise (West = -X)
        let pos_2x1 = PaintingEntity::calculate_center_pos(location, BlockDirection::North, 2, 1);
        assert_eq!(pos_2x1.x, 10.0);
        assert_eq!(pos_2x1.y, 64.5);
        assert!((pos_2x1.z - 19.96875).abs() < 1e-6);

        // 2x2 painting facing North: height is 2, shifts +0.5 along Y
        let pos_2x2 = PaintingEntity::calculate_center_pos(location, BlockDirection::North, 2, 2);
        assert_eq!(pos_2x2.x, 10.0);
        assert_eq!(pos_2x2.y, 65.0);
        assert!((pos_2x2.z - 19.96875).abs() < 1e-6);

        // 4x4 painting facing North
        let pos_4x4 = PaintingEntity::calculate_center_pos(location, BlockDirection::North, 4, 4);
        assert_eq!(pos_4x4.x, 10.0);
        assert_eq!(pos_4x4.y, 65.0);
        assert!((pos_4x4.z - 19.96875).abs() < 1e-6);

        // 1x1 painting facing South: placed on south face of (10, 64, 20) -> target is (10, 64, 21)
        let pos_south = PaintingEntity::calculate_center_pos(location, BlockDirection::South, 1, 1);
        assert_eq!(pos_south.x, 10.5);
        assert_eq!(pos_south.y, 64.5);
        assert!((pos_south.z - 21.03125).abs() < 1e-6);
    }

    #[test]
    fn variant_resolution() {
        assert_eq!(
            PaintingVariant::from_name("minecraft:kebab"),
            Some(PaintingVariant::Kebab)
        );
        assert_eq!(
            PaintingVariant::from_name("kebab"),
            Some(PaintingVariant::Kebab)
        );
        assert_eq!(PaintingVariant::from_name("invalid"), None);
        assert_eq!(PaintingVariant::Kebab.asset_id(), "minecraft:kebab");
    }

    fn state_grid(
        walls: Vec<BlockPos>,
        wall: &'static pumpkin_data::BlockState,
    ) -> impl Fn(BlockPos) -> &'static pumpkin_data::BlockState {
        let air = pumpkin_data::Block::AIR.default_state;
        move |pos| {
            if walls.contains(&pos) { wall } else { air }
        }
    }

    #[test]
    fn one_block_stone_face_with_air_accepts_a_painting() {
        let location = BlockPos(Vector3::new(0, 64, 0));
        let at = state_grid(vec![location], pumpkin_data::Block::STONE.default_state);
        assert!(PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::North,
            1,
            1,
            &at
        ));
        assert!(PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::East,
            1,
            1,
            &at
        ));
        let area = PaintingVariant::all_placeable()
            .iter()
            .filter(|variant| {
                PaintingEntity::painting_blocks_fit(
                    location,
                    BlockDirection::North,
                    variant.width(),
                    variant.height(),
                    &at,
                )
            })
            .map(|variant| variant.width() * variant.height())
            .max();
        assert_eq!(area, Some(1));
    }

    #[test]
    fn painting_rejects_open_air_blocked_front_and_vertical_faces() {
        let location = BlockPos(Vector3::new(0, 64, 0));
        let air = state_grid(Vec::new(), pumpkin_data::Block::STONE.default_state);
        assert!(!PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::North,
            1,
            1,
            &air
        ));

        let stone = pumpkin_data::Block::STONE.default_state;
        let blocked = |pos: BlockPos| {
            if pos == location || pos == location.offset(BlockDirection::North.to_offset()) {
                stone
            } else {
                pumpkin_data::Block::AIR.default_state
            }
        };
        assert!(!PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::North,
            1,
            1,
            blocked
        ));
        let solid = state_grid(vec![location], stone);
        assert!(!PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::Up,
            1,
            1,
            &solid
        ));
        assert!(!PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::Down,
            1,
            1,
            &solid
        ));
    }

    #[test]
    fn painting_rejects_slab_and_snow_faces_that_are_not_sturdy() {
        let location = BlockPos(Vector3::new(2, 70, -3));
        for block in [pumpkin_data::Block::OAK_SLAB, pumpkin_data::Block::SNOW] {
            let at = state_grid(vec![location], block.default_state);
            assert!(
                !PaintingEntity::painting_blocks_fit(location, BlockDirection::North, 1, 1, &at),
                "{}",
                block.name
            );
        }
    }

    #[test]
    fn wider_painting_needs_every_covered_face_clear() {
        let location = BlockPos(Vector3::new(0, 0, 0));
        let face = BlockDirection::North;
        let cells = PaintingEntity::covered_cells(location, face, 2, 2);
        assert_eq!(cells.len(), 4);
        let walls: Vec<BlockPos> = cells.iter().map(|(wall, _)| *wall).collect();
        let at = state_grid(walls, pumpkin_data::Block::STONE.default_state);
        assert!(PaintingEntity::painting_blocks_fit(
            location, face, 2, 2, &at
        ));
        assert!(!PaintingEntity::painting_blocks_fit(
            location, face, 4, 4, &at
        ));
        let only_clicked = state_grid(vec![location], pumpkin_data::Block::STONE.default_state);
        assert!(!PaintingEntity::painting_blocks_fit(
            location,
            face,
            2,
            2,
            &only_clicked
        ));
    }

    #[test]
    fn repeater_side_supports_a_painting_and_its_top_does_not() {
        let location = BlockPos(Vector3::new(1, 2, 3));
        let repeater = pumpkin_data::Block::REPEATER.default_state;
        let air = pumpkin_data::Block::AIR.default_state;
        let at = |pos: BlockPos| {
            if pos == location { repeater } else { air }
        };
        assert!(hanging_surface_supports(
            BlockDirection::North,
            repeater,
            air
        ));
        assert!(!hanging_surface_supports(BlockDirection::Up, repeater, air));
        assert!(PaintingEntity::painting_blocks_fit(
            location,
            BlockDirection::North,
            1,
            1,
            at
        ));
    }

    #[test]
    fn anchor_round_trips_through_the_placed_center() {
        let location = BlockPos(Vector3::new(10, 64, 20));
        for (face, width, height) in [
            (BlockDirection::North, 1, 1),
            (BlockDirection::North, 2, 2),
            (BlockDirection::South, 4, 3),
            (BlockDirection::East, 3, 2),
            (BlockDirection::West, 2, 1),
        ] {
            let center = PaintingEntity::calculate_center_pos(location, face, width, height);
            assert_eq!(
                PaintingEntity::anchor_from_center(center, face, width, height),
                Some(location)
            );
        }
    }

    #[test]
    fn survival_break_drops_one_painting_and_creative_drops_none() {
        assert!(painting_should_drop(true, false));
        assert!(!painting_should_drop(true, true));
        assert!(!painting_should_drop(false, false));
        assert!(!painting_should_drop(false, true));
    }
}
