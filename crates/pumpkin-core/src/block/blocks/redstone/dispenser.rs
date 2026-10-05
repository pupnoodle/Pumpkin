use rand::{Rng, RngExt, rng};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

use crate::block::blocks::carved_pumpkin::find_golem_pattern;
use crate::block::blocks::redstone::block_receives_redstone_power;
use crate::block::blocks::tnt::TNTBlock;
use crate::block::blocks::wither_skull::find_wither_pattern;
use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, GetComparatorOutputArgs, GetScreenHandlerFactoryArgs, NormalUseArgs,
    OnNeighborUpdateArgs, OnPlaceArgs, OnScheduledTickArgs, PlacedArgs,
};
use crate::entity::ageable::AgeableMob;
use crate::entity::decoration::armor_stand::ArmorStandEntity;
use crate::entity::decoration::leash_knot::LeashKnotEntity;
use crate::entity::experience_orb::ExperienceOrbEntity;
use crate::entity::item::ItemEntity;
use crate::entity::passive::axolotl::AxolotlEntity;
use crate::entity::passive::mooshroom::{MooshroomEntity, MooshroomVariant};
use crate::entity::passive::sheep::SheepEntity;
use crate::entity::passive::snow_golem::SnowGolemEntity;
use crate::entity::projectile::arrow::{ArrowEntity, ArrowPickup};
use crate::entity::projectile::egg::EggEntity;
use crate::entity::projectile::firework_rocket::FireworkRocketEntity;
use crate::entity::projectile::lingering_potion::LingeringPotionEntity;
use crate::entity::projectile::small_fireball::SmallFireballEntity;
use crate::entity::projectile::snowball::SnowballEntity;
use crate::entity::projectile::splash_potion::SplashPotionEntity;
use crate::entity::projectile::wind_charge::{WIND_CHARGE_GRAVITY, WindChargeEntity};
use crate::entity::projectile::{ProjectileHit, ThrownItemEntity};
use crate::entity::tnt::TNTEntity;
use crate::entity::r#type::from_type;
use crate::entity::vehicle::boat::BoatEntity;
use crate::entity::vehicle::minecart::MinecartEntity;
use crate::entity::{Entity, EntityBase};
use crate::item::ItemMetadata;
use crate::item::items::boat::BoatItem;
use crate::item::items::bucket::{
    FilledBucketItem, play_bucket_evaporation, should_evaporate_in_nether, try_pickup_fluid_at,
    try_place_filled_bucket,
};
use crate::item::items::honeycomb::try_wax_block;
use crate::item::items::ignite::ignition::Ignition;
use crate::item::items::minecart::MinecartItem;
use crate::item::items::spawn_egg::prepare_egg_mob;
use crate::plugin::api::events::entity::creature_spawn::CreatureSpawnReason;
use crate::world::World;

use crate::block::entities::dispenser::DispenserBlockEntity;
use pumpkin_data::block_properties::{
    BeeNestLikeProperties, BlockProperties, EndRodLikeProperties, Facing,
    PoweredRailLikeProperties, RailLikeProperties, RespawnAnchorLikeProperties,
    SkeletonSkullLikeProperties,
};
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{EquippableImpl, IDSet, PotionContentsImpl};
use pumpkin_data::entity::{EntityStatus, EntityType, entity_from_egg};
use pumpkin_data::fluid::Fluid;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::particle::Particle;
use pumpkin_data::potion::Potion;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::translation;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockStateId, FacingExt};
use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::create_generic_3x3;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block;
use pumpkin_protocol::bedrock::server::actor_event::ActorEventID;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;
use pumpkin_util::math::boundingbox::{BoundingBox, EntityDimensions};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::math::wrap_degrees;
use pumpkin_util::text::TextComponent;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;

struct DispenserScreenFactory(Arc<dyn Inventory>);

impl ScreenHandlerFactory for DispenserScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let handler = create_generic_3x3(sync_id, player_inventory, self.0.clone(), player);
        let screen_handler_arc = Arc::new(Mutex::new(handler));

        Some(screen_handler_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        pumpkin_macros::translate_cross!(
            translation::java::CONTAINER_DISPENSER,
            translation::bedrock::CONTAINER_DISPENSER
        )
    }
}

#[pumpkin_block("minecraft:dispenser")]
pub struct DispenserBlock;

type DispenserLikeProperties = pumpkin_data::block_properties::DispenserLikeProperties;

struct DispenseContext<'a> {
    world: &'a Arc<World>,
    position: &'a BlockPos,
    facing: Facing,
}

fn triangle<R: Rng>(rng: &mut R, min: f64, max: f64) -> f64 {
    (rng.random::<f64>() - rng.random::<f64>()).mul_add(max, min)
}

const fn to_normal(facing: Facing) -> Vector3<f64> {
    match facing {
        Facing::North => Vector3::new(0., 0., -1.),
        Facing::East => Vector3::new(1., 0., 0.),
        Facing::South => Vector3::new(0., 0., 1.),
        Facing::West => Vector3::new(-1., 0., 0.),
        Facing::Up => Vector3::new(0., 1., 0.),
        Facing::Down => Vector3::new(0., -1., 0.),
    }
}

const fn to_data3d(facing: Facing) -> i32 {
    match facing {
        Facing::North => 2,
        Facing::East => 5,
        Facing::South => 3,
        Facing::West => 4,
        Facing::Up => 1,
        Facing::Down => 0,
    }
}

const fn to_rotation16(facing: Facing) -> u8 {
    match facing {
        Facing::South | Facing::Up | Facing::Down => 0,
        Facing::West => 4,
        Facing::North => 8,
        Facing::East => 12,
    }
}

fn is_allowed_entity(
    allowed: Option<&IDSet<EntityType>>,
    entity_type: &'static EntityType,
) -> bool {
    match allowed {
        None => true,
        Some(IDSet::Tag(tag)) => entity_type.is_tagged_with(tag).unwrap_or(false),
        Some(IDSet::IDs(types)) => types.contains(&entity_type),
    }
}

const fn wool_of_color(color: u8) -> &'static Item {
    match color {
        1 => &Item::ORANGE_WOOL,
        2 => &Item::MAGENTA_WOOL,
        3 => &Item::LIGHT_BLUE_WOOL,
        4 => &Item::YELLOW_WOOL,
        5 => &Item::LIME_WOOL,
        6 => &Item::PINK_WOOL,
        7 => &Item::GRAY_WOOL,
        8 => &Item::LIGHT_GRAY_WOOL,
        9 => &Item::CYAN_WOOL,
        10 => &Item::PURPLE_WOOL,
        11 => &Item::BLUE_WOOL,
        12 => &Item::BROWN_WOOL,
        13 => &Item::GREEN_WOOL,
        14 => &Item::RED_WOOL,
        15 => &Item::BLACK_WOOL,
        _ => &Item::WHITE_WOOL,
    }
}

fn water_bottle() -> ItemStack {
    ItemStack::new_with_component(
        1,
        &Item::POTION,
        vec![(
            DataComponent::PotionContents,
            Some(Box::new(PotionContentsImpl {
                potion_id: Some(i32::from(Potion::WATER.id)),
                custom_color: None,
                custom_effects: Vec::new(),
                custom_name: None,
            }) as Box<_>),
        )],
    )
}

fn is_water_bottle(stack: &ItemStack) -> bool {
    stack.item.id == Item::POTION.id
        && stack
            .get_data_component::<PotionContentsImpl>()
            .and_then(|contents| contents.potion_id)
            == Some(i32::from(Potion::WATER.id))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectItemUse {
    BoneMeal,
    ExperienceBottle,
}

const fn direct_item_use(item_id: u16) -> Option<DirectItemUse> {
    if item_id == Item::BONE_MEAL.id {
        Some(DirectItemUse::BoneMeal)
    } else if item_id == Item::EXPERIENCE_BOTTLE.id {
        Some(DirectItemUse::ExperienceBottle)
    } else {
        None
    }
}

const EXPERIENCE_BOTTLE_MIN: u32 = 3;
const EXPERIENCE_BOTTLE_SPAN: u32 = 9;
const EXPERIENCE_BOTTLE_GRAVITY: f64 = 0.07;

const fn experience_bottle_amount(roll: u32) -> u32 {
    EXPERIENCE_BOTTLE_MIN + roll % EXPERIENCE_BOTTLE_SPAN
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BucketMob {
    Axolotl,
    Cod,
    Salmon,
    Pufferfish,
    TropicalFish,
    Tadpole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BucketPlace {
    Drop,
    Empty { mob: Option<BucketMob> },
}

const fn bucket_mob(item_id: u16) -> Option<BucketMob> {
    if item_id == Item::AXOLOTL_BUCKET.id {
        Some(BucketMob::Axolotl)
    } else if item_id == Item::COD_BUCKET.id {
        Some(BucketMob::Cod)
    } else if item_id == Item::SALMON_BUCKET.id {
        Some(BucketMob::Salmon)
    } else if item_id == Item::PUFFERFISH_BUCKET.id {
        Some(BucketMob::Pufferfish)
    } else if item_id == Item::TROPICAL_FISH_BUCKET.id {
        Some(BucketMob::TropicalFish)
    } else if item_id == Item::TADPOLE_BUCKET.id {
        Some(BucketMob::Tadpole)
    } else {
        None
    }
}

const fn bucket_mob_entity(mob: BucketMob) -> &'static EntityType {
    match mob {
        BucketMob::Axolotl => &EntityType::AXOLOTL,
        BucketMob::Cod => &EntityType::COD,
        BucketMob::Salmon => &EntityType::SALMON,
        BucketMob::Pufferfish => &EntityType::PUFFERFISH,
        BucketMob::TropicalFish => &EntityType::TROPICAL_FISH,
        BucketMob::Tadpole => &EntityType::TADPOLE,
    }
}

const fn bucket_place(item_id: u16, placed: bool, evaporated: bool) -> BucketPlace {
    if placed {
        BucketPlace::Empty {
            mob: if evaporated {
                None
            } else {
                bucket_mob(item_id)
            },
        }
    } else {
        BucketPlace::Drop
    }
}

const fn bucket_empty_sound(item_id: u16) -> Sound {
    if item_id == Item::AXOLOTL_BUCKET.id {
        Sound::ItemBucketEmptyAxolotl
    } else if item_id == Item::TADPOLE_BUCKET.id {
        Sound::ItemBucketEmptyTadpole
    } else if item_id == Item::LAVA_BUCKET.id {
        Sound::ItemBucketEmptyLava
    } else if item_id == Item::POWDER_SNOW_BUCKET.id {
        Sound::ItemBucketEmptyPowderSnow
    } else if bucket_mob(item_id).is_some() {
        Sound::ItemBucketEmptyFish
    } else {
        Sound::ItemBucketEmpty
    }
}

fn bucket_mob_spawn(pos: BlockPos) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShearEffect {
    Wool,
    Mushrooms,
    Pumpkin,
    BreakKnot,
    CutLeash,
    None,
}

struct ShearState {
    baby: bool,
    sheared: bool,
    has_pumpkin: bool,
    leashed: bool,
}

fn shear_effect(resource_name: &str, state: &ShearState) -> ShearEffect {
    match resource_name {
        "sheep" if !state.baby && !state.sheared => ShearEffect::Wool,
        "mooshroom" if !state.baby => ShearEffect::Mushrooms,
        "snow_golem" if state.has_pumpkin => ShearEffect::Pumpkin,
        "leash_knot" => ShearEffect::BreakKnot,
        _ if state.leashed => ShearEffect::CutLeash,
        _ => ShearEffect::None,
    }
}

struct ThrownExperienceBottle {
    thrown: ThrownItemEntity,
}

impl ThrownExperienceBottle {
    const fn new(entity: Entity) -> Self {
        Self {
            thrown: ThrownItemEntity {
                entity,
                owner_id: None,
                collides_with_projectiles: false,
                has_hit: AtomicBool::new(false),
                gravity: EXPERIENCE_BOTTLE_GRAVITY,
            },
        }
    }
}

impl EntityBase for ThrownExperienceBottle {
    fn get_entity(&self) -> &Entity {
        &self.thrown.entity
    }

    fn get_living_entity(&self) -> Option<&crate::entity::living::LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &crate::server::Server) {
        self.thrown.process_tick(caller);
    }

    fn init_data_tracker(&self) {
        self.thrown.entity.set_synced_data(
            pumpkin_data::tracked_data::thrown_experience_bottle::ITEM_STACK,
            ItemStackSerializer::from(ItemStack::new(1, &Item::EXPERIENCE_BOTTLE)),
        );
    }

    fn on_hit(&self, hit: ProjectileHit) {
        let world = self.thrown.entity.world.load();
        world.send_entity_status(
            &self.thrown.entity,
            EntityStatus::Death,
            Some(ActorEventID::Death),
        );
        ExperienceOrbEntity::spawn(
            &world,
            hit.hit_pos(),
            experience_bottle_amount(rng().random()),
        );
    }
}

impl BlockBehaviour for DispenserBlock {
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::InspectDispenser as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
        }
        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        let block_entity = args.world.get_block_entity(args.position)?;
        let inventory = block_entity.get_inventory()?;
        Some(Box::new(DispenserScreenFactory(inventory)))
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = DispenserLikeProperties::default(args.block);
        props.facing = args.player.get_entity().get_facing().opposite();
        props.to_state_id(args.block)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let dispenser_block_entity = DispenserBlockEntity::new(*args.position);
        args.world
            .add_block_entity(Arc::new(dispenser_block_entity));
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let powered = block_receives_redstone_power(args.world, args.position)
            || block_receives_redstone_power(args.world, &args.position.up());

        let mut props =
            DispenserLikeProperties::from_state_id(args.world.get_block_state(args.position).id);

        if powered && !props.triggered {
            args.world
                .schedule_block_tick(args.block, *args.position, 4, TickPriority::Normal);
            props.triggered = true;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_LISTENERS,
            );
        } else if !powered && props.triggered {
            props.triggered = false;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_LISTENERS,
            );
        }
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let (_block, state) = args.world.get_block_and_state(args.position);
        if let Some(block_entity) = args.world.get_block_entity(args.position) {
            let Some(dispenser) = block_entity.as_any().downcast_ref::<DispenserBlockEntity>()
            else {
                return;
            };

            if let Some((slot_index, mut item)) = dispenser.get_random_slot() {
                let props = DispenserLikeProperties::from_state_id(state.id);
                let ctx = DispenseContext {
                    world: args.world,
                    position: args.position,
                    facing: props.facing,
                };
                Self::dispense(&ctx, dispenser, &mut item);
                dispenser.set_stack(slot_index, item);
            } else {
                args.world
                    .sync_world_event(WorldEvent::SoundDispenserFail, *args.position, 0);
            }
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        crate::block::container_comparator_output(&args)
    }
}

impl DispenserBlock {
    // Velocity values match the vanilla dispenser projectile settings.
    const DEFAULT_PROJECTILE_POWER: f64 = 1.1;
    const DEFAULT_PROJECTILE_UNCERTAINTY: f64 = 6.0;
    const POTION_PROJECTILE_POWER: f64 = 1.375;
    const POTION_PROJECTILE_UNCERTAINTY: f64 = 3.0;
    // Fire charges and wind charges share these values.
    const FIREBALL_PROJECTILE_POWER: f64 = 1.0;
    const FIREBALL_PROJECTILE_UNCERTAINTY: f64 = 6.666_666_5;
    const FIREWORK_PROJECTILE_POWER: f64 = 0.5;
    const FIREWORK_PROJECTILE_UNCERTAINTY: f64 = 1.0;

    fn dispense(ctx: &DispenseContext<'_>, dispenser: &DispenserBlockEntity, item: &mut ItemStack) {
        let mut event = crate::plugin::api::events::block::block_dispense::BlockDispenseEvent::new(
            *ctx.position,
            item.item.registry_key.to_string(),
        );
        if let Some(server) = ctx.world.server.upgrade() {
            server.plugin_manager.fire_blocking(&server, &mut event);
        }
        if event.cancelled {
            ctx.world
                .sync_world_event(WorldEvent::SoundDispenserFail, *ctx.position, 0);
            return;
        }

        // Still missing some specific dispenser behavior that you can find here:
        // https://minecraft.wiki/w/Dispenser#Usage
        let arrows = [
            Item::ARROW.id,
            Item::TIPPED_ARROW.id,
            Item::SPECTRAL_ARROW.id,
        ];
        let boats = BoatItem::ids();

        if arrows.contains(&item.item.id) {
            // Arrows
            Self::fire_arrow(ctx, item);
        } else if boats.contains(&item.item.id) {
            // Boats
            if !Self::dispense_boat(ctx, item) {
                Self::drop_item(ctx, item);
            }
        } else if MinecartItem::ids().contains(&item.item.id) {
            // Minecarts
            if !Self::dispense_minecart(ctx, item) {
                Self::drop_item(ctx, item);
            }
        } else if item.item.id == Item::ARMOR_STAND.id {
            // Armor stands
            if !Self::dispense_armor_stand(ctx, item) {
                Self::drop_item(ctx, item);
            }
        } else if item.item.id == Item::TNT.id {
            // TNT
            Self::dispense_tnt(ctx, item);
        } else if item.item.id == Item::SNOWBALL.id {
            Self::dispense_snowball(ctx, item);
        } else if item.item.id == Item::EGG.id {
            Self::dispense_egg(ctx, item);
        } else if item.item.id == Item::SPLASH_POTION.id {
            Self::dispense_splash_potion(ctx, item);
        } else if item.item.id == Item::LINGERING_POTION.id {
            Self::dispense_lingering_potion(ctx, item);
        } else if item.item.id == Item::FIRE_CHARGE.id {
            Self::dispense_fire_charge(ctx, item);
        } else if item.item.id == Item::WIND_CHARGE.id {
            Self::dispense_wind_charge(ctx, item);
        } else if item.item.id == Item::FIREWORK_ROCKET.id {
            Self::dispense_firework_rocket(ctx, item);
        } else if item.item.id == Item::BUCKET.id {
            // Empty buckets pick up the fluid in front of the dispenser
            Self::dispense_empty_bucket(ctx, dispenser, item);
        } else if FilledBucketItem::ids().contains(&item.item.id) {
            // Filled buckets place their fluid in front of the dispenser
            Self::dispense_filled_bucket(ctx, dispenser, item);
        } else if item.item.id == Item::FLINT_AND_STEEL.id {
            // Flint and steel light fires and prime TNT
            Self::dispense_flint_and_steel(ctx, item);
        } else if item.item.id == Item::HONEYCOMB.id {
            // Honeycombs wax copper blocks
            Self::dispense_honeycomb(ctx, item);
        } else if entity_from_egg(item.item.id).is_some() {
            // Spawn eggs
            Self::dispense_spawn_egg(ctx, item);
        } else if item.item.id == Item::SHEARS.id {
            // Shears carve pumpkins, harvest full beehives and shear sheep, mooshrooms,
            // snow golems, and leads
            Self::dispense_shears(ctx, item);
        } else if item.item.id == Item::GLASS_BOTTLE.id {
            // Glass bottles fill from water and full beehives
            if !Self::dispense_glass_bottle(ctx, dispenser, item) {
                Self::drop_item_with_sound(ctx, item, WorldEvent::SoundDispenserFail);
            }
        } else if is_water_bottle(item) {
            // Water bottles convert dirt-likes into mud
            if !Self::dispense_water_bottle(ctx, item) {
                Self::drop_item(ctx, item);
            }
        } else if item.item.id == Item::GLOWSTONE.id {
            match Self::dispense_glowstone(ctx, item) {
                Some(true) => Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense),
                Some(false) => Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail),
                None => Self::drop_item(ctx, item),
            }
        } else if item.item.id == Item::WITHER_SKELETON_SKULL.id {
            // Placed only when it completes a wither, otherwise worn as a helmet
            Self::dispense_mob_head(ctx, item, &Block::WITHER_SKELETON_SKULL);
        } else if item.item.id == Item::CARVED_PUMPKIN.id {
            // Placed only when it completes a golem, otherwise worn as a helmet
            Self::dispense_mob_head(ctx, item, &Block::CARVED_PUMPKIN);
        } else if Block::from_item_id(item.item.id)
            .is_some_and(|block| block.has_tag(&tag::Block::MINECRAFT_SHULKER_BOXES))
        {
            // Shulker boxes place themselves
            if !Self::dispense_shulker_box(ctx, item) {
                Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
            }
        } else if Self::dispense_equipment(ctx, item) {
            // Armor, elytra, heads, saddles, horse/wolf armor and llama carpets
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else if let Some(kind) = direct_item_use(item.item.id) {
            match kind {
                DirectItemUse::BoneMeal => Self::dispense_bone_meal(ctx, item),
                DirectItemUse::ExperienceBottle => Self::dispense_experience_bottle(ctx, item),
            }
        } else {
            // TODO: Chests onto llamas, brushes onto armadillos
            Self::drop_item(ctx, item);
        }
    }

    fn projectile_spawn_position(ctx: &DispenseContext<'_>) -> Vector3<f64> {
        ctx.position
            .to_centered_f64()
            .add(&(to_normal(ctx.facing) * 0.7))
    }

    fn launch_thrown(
        ctx: &DispenseContext<'_>,
        thrown: &ThrownItemEntity,
        power: f64,
        uncertainty: f64,
    ) {
        let facing = to_normal(ctx.facing);
        thrown.set_velocity(facing.x, facing.y + 0.1, facing.z, power, uncertainty);
    }

    fn finish_projectile_launch(
        ctx: &DispenseContext<'_>,
        projectile: Arc<dyn EntityBase>,
        launch_event: WorldEvent,
    ) {
        ctx.world.spawn_entity(projectile);
        Self::play_dispense_effects(ctx, launch_event);
    }

    fn play_dispense_effects(ctx: &DispenseContext<'_>, sound_event: WorldEvent) {
        ctx.world.sync_world_event(sound_event, *ctx.position, 0);
        ctx.world.sync_world_event(
            WorldEvent::ParticlesShootSmoke,
            *ctx.position,
            to_data3d(ctx.facing),
        );
    }

    fn fire_arrow(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let projectile = item.split(1);

        let facing = to_normal(ctx.facing);
        let arrow_entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            ArrowEntity::entity_type_for_item(projectile.item),
        );
        let arrow =
            ArrowEntity::new_with_item(arrow_entity, None, &projectile, ArrowPickup::Allowed);
        arrow.apply_on_projectile_spawned(&projectile);

        arrow.set_velocity(
            facing.x,
            facing.y + 0.1,
            facing.z,
            Self::DEFAULT_PROJECTILE_POWER,
            Self::DEFAULT_PROJECTILE_UNCERTAINTY,
        );

        Self::finish_projectile_launch(
            ctx,
            Arc::new(arrow),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn target_position(ctx: &DispenseContext<'_>) -> BlockPos {
        let facing = to_normal(ctx.facing);
        ctx.position.offset(Vector3::new(
            facing.x as i32,
            facing.y as i32,
            facing.z as i32,
        ))
    }

    fn has_room_for(
        ctx: &DispenseContext<'_>,
        spawn_pos: Vector3<f64>,
        size: &EntityDimensions,
    ) -> bool {
        let bounding_box = BoundingBox::new_from_pos(spawn_pos.x, spawn_pos.y, spawn_pos.z, size);
        ctx.world.is_space_empty(bounding_box)
            && ctx.world.get_entities_at_box(&bounding_box).is_empty()
    }

    fn dispense_boat(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        let target = Self::target_position(ctx);
        let is_water = |id: u16| id == Fluid::WATER.id || id == Fluid::FLOWING_WATER.id;

        let spawn_pos = if is_water(ctx.world.get_fluid(&target).id) {
            target.to_f64()
        } else if ctx.world.get_block_state(&target).is_air()
            && is_water(ctx.world.get_fluid(&target.down()).id)
        {
            target.down().to_f64()
        } else {
            return false;
        };

        let entity_type = BoatItem::item_to_entity(item.item);
        let dimensions = EntityDimensions::new(
            entity_type.dimension[0],
            entity_type.dimension[1],
            entity_type.eye_height,
        );
        if !Self::has_room_for(ctx, spawn_pos, &dimensions) {
            return false;
        }

        let _ = item.split(1);
        let facing = to_normal(ctx.facing);
        let entity = Entity::new(ctx.world.clone(), spawn_pos, entity_type);
        entity.set_rotation(facing.x.atan2(facing.z) as f32 * 57.295_776, 0.0);
        ctx.world.spawn_entity(Arc::new(BoatEntity::new(entity)));

        ctx.world
            .sync_world_event(WorldEvent::SoundDispenserDispense, *ctx.position, 0);
        true
    }

    fn dispense_armor_stand(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        let target = Self::target_position(ctx);
        let spawn_pos = target.to_f64();
        let dimensions = EntityDimensions::new(
            EntityType::ARMOR_STAND.dimension[0],
            EntityType::ARMOR_STAND.dimension[1],
            EntityType::ARMOR_STAND.eye_height,
        );
        if !Self::has_room_for(ctx, spawn_pos, &dimensions) {
            return false;
        }

        let _ = item.split(1);
        let facing = to_normal(ctx.facing);
        let entity = Entity::new(ctx.world.clone(), spawn_pos, &EntityType::ARMOR_STAND);
        entity.set_rotation(facing.x.atan2(facing.z) as f32 * 57.295_776, 0.0);

        ctx.world.play_sound(
            Sound::EntityArmorStandPlace,
            SoundCategory::Blocks,
            &spawn_pos,
        );
        ctx.world
            .spawn_entity(Arc::new(ArmorStandEntity::new(entity)));

        ctx.world
            .sync_world_event(WorldEvent::SoundDispenserDispense, *ctx.position, 0);
        true
    }

    fn dispense_tnt(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        // Vanilla keeps the item and plays the fail click when TNT is disabled.
        if !ctx.world.level_info.load().game_rules.tnt_explodes {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
            return;
        }

        let _ = item.split(1);
        let target = Self::target_position(ctx);

        let tnt = TNTEntity::primed(ctx.world, &target, TNTEntity::DEFAULT_FUSE);
        let spawn_pos = tnt.get_entity().pos.load();
        ctx.world.spawn_entity(tnt);
        ctx.world
            .play_sound(Sound::EntityTntPrimed, SoundCategory::Blocks, &spawn_pos);
        ctx.world
            .emit_game_event(GameEvent::EntityPlace.name(), target.to_centered_f64());

        Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
    }

    fn dispense_spawn_egg(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let Some(entity_type) = entity_from_egg(item.item.id) else {
            return;
        };

        let spawn_pos = Self::target_position(ctx).to_f64();

        let mob = from_type(entity_type, spawn_pos, ctx.world, Uuid::new_v4());
        let yaw = wrap_degrees(rng().random::<f32>() * 360.0) % 360.0;
        mob.get_entity().set_rotation(yaw, 0.0);
        // A dispenser has no acting player, matching vanilla's null `user` for this source.
        prepare_egg_mob(item, &mob, ctx.world, None);

        // Vanilla SpawnEggItemBehavior keeps the egg when nothing spawned.
        if ctx
            .world
            .spawn_creature(mob, CreatureSpawnReason::DispenseEgg, None)
        {
            item.decrement(1);
        }

        ctx.world
            .sync_world_event(WorldEvent::SoundDispenserDispense, *ctx.position, 0);
    }

    fn dispense_snowball(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let _ = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::SNOWBALL,
        );
        let snowball = SnowballEntity::new(entity);
        Self::launch_thrown(
            ctx,
            &snowball.thrown,
            Self::DEFAULT_PROJECTILE_POWER,
            Self::DEFAULT_PROJECTILE_UNCERTAINTY,
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(snowball),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn dispense_egg(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let projectile = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::EGG,
        );
        let egg = EggEntity::new(entity);
        egg.set_item_stack(projectile);
        Self::launch_thrown(
            ctx,
            &egg.thrown,
            Self::DEFAULT_PROJECTILE_POWER,
            Self::DEFAULT_PROJECTILE_UNCERTAINTY,
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(egg),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn dispense_splash_potion(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let projectile = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::SPLASH_POTION,
        );
        let potion = SplashPotionEntity::new(entity);
        potion.set_item_stack(projectile);
        Self::launch_thrown(
            ctx,
            &potion.thrown,
            Self::POTION_PROJECTILE_POWER,
            Self::POTION_PROJECTILE_UNCERTAINTY,
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(potion),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn dispense_lingering_potion(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let projectile = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::LINGERING_POTION,
        );
        let potion = LingeringPotionEntity::new(entity);
        potion.set_item_stack(projectile);
        Self::launch_thrown(
            ctx,
            &potion.thrown,
            Self::POTION_PROJECTILE_POWER,
            Self::POTION_PROJECTILE_UNCERTAINTY,
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(potion),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn dispense_fire_charge(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let _ = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::SMALL_FIREBALL,
        );
        let fireball = SmallFireballEntity::new(entity);
        let facing = to_normal(ctx.facing);
        let dir = Vector3::new(
            triangle(&mut rng(), facing.x, 0.114_850_000_000_000_01),
            triangle(&mut rng(), facing.y, 0.114_850_000_000_000_01),
            triangle(&mut rng(), facing.z, 0.114_850_000_000_000_01),
        )
        .normalize();
        fireball.get_entity().set_velocity(dir);
        let len = dir.horizontal_length();
        fireball.get_entity().set_rotation(
            dir.x.atan2(dir.z) as f32 * 57.295_776,
            dir.y.atan2(len) as f32 * 57.295_776,
        );
        Self::finish_projectile_launch(ctx, Arc::new(fireball), WorldEvent::SoundBlazeFireball);
    }

    fn dispense_wind_charge(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let _ = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::WIND_CHARGE,
        );
        let thrown = ThrownItemEntity {
            entity,
            owner_id: None,
            collides_with_projectiles: false,
            has_hit: AtomicBool::new(false),
            gravity: WIND_CHARGE_GRAVITY,
        };
        Self::launch_thrown(
            ctx,
            &thrown,
            Self::FIREBALL_PROJECTILE_POWER,
            Self::FIREBALL_PROJECTILE_UNCERTAINTY,
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(WindChargeEntity::new_normal(thrown)),
            WorldEvent::SoundWindChargeShoot,
        );
    }

    fn dispense_firework_rocket(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let _ = item.split(1);
        let facing = to_normal(ctx.facing);
        // Vanilla spawns fireworks closer to the dispenser face and slightly above center.
        let position = ctx
            .position
            .to_centered_f64()
            .add(&(facing * (0.7 * 0.5125)))
            .add(&Vector3::new(0.0, 0.08, 0.0));
        let entity = Entity::new(ctx.world.clone(), position, &EntityType::FIREWORK_ROCKET);
        let rocket = FireworkRocketEntity::new(entity);

        // `FireworkRocketEntity` does not expose its inner projectile, so replicate
        // `ThrownItemEntity::set_velocity` here.
        let deviation = 0.017_227_5 * Self::FIREWORK_PROJECTILE_UNCERTAINTY;
        let velocity = Vector3::new(facing.x, facing.y + 0.1, facing.z)
            .normalize()
            .add_raw(
                triangle(&mut rng(), 0.0, deviation),
                triangle(&mut rng(), 0.0, deviation),
                triangle(&mut rng(), 0.0, deviation),
            )
            .multiply(
                Self::FIREWORK_PROJECTILE_POWER,
                Self::FIREWORK_PROJECTILE_POWER,
                Self::FIREWORK_PROJECTILE_POWER,
            );
        let rocket_entity = rocket.get_entity();
        rocket_entity.set_velocity(velocity);
        rocket_entity.set_rotation(
            velocity.x.atan2(velocity.z) as f32 * 57.295_776,
            velocity.y.atan2(velocity.horizontal_length()) as f32 * 57.295_776,
        );

        Self::finish_projectile_launch(ctx, Arc::new(rocket), WorldEvent::SoundFireworkShoot);
    }

    fn dispense_empty_bucket(
        ctx: &DispenseContext<'_>,
        dispenser: &DispenserBlockEntity,
        item: &mut ItemStack,
    ) {
        let front = Self::target_position(ctx);
        let Some(filled) = try_pickup_fluid_at(ctx.world, front) else {
            Self::drop_item(ctx, item);
            return;
        };

        Self::consume_with_remainder(ctx, dispenser, item, ItemStack::new(1, filled));
        Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
    }

    /// Places `stack` into the first empty slot, returning it back if every slot is occupied.
    /// The slot currently being dispensed from still holds its pre-dispense stack, so it is
    /// never considered free.
    fn add_to_first_free_slot(
        dispenser: &DispenserBlockEntity,
        stack: ItemStack,
    ) -> Option<ItemStack> {
        let mut items = dispenser
            .items
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for slot in items.iter_mut() {
            if slot.is_empty() {
                *slot = stack;
                dispenser.mark_dirty();
                return None;
            }
        }
        Some(stack)
    }

    fn dispense_filled_bucket(
        ctx: &DispenseContext<'_>,
        dispenser: &DispenserBlockEntity,
        item: &mut ItemStack,
    ) {
        let front = Self::target_position(ctx);
        let evaporated = should_evaporate_in_nether(item.item, ctx.world);
        let placed = if evaporated {
            play_bucket_evaporation(ctx.world, &front.to_f64());
            true
        } else {
            try_place_filled_bucket(
                ctx.world,
                item.item,
                *ctx.position,
                ctx.facing.to_block_direction(),
            )
        };

        match bucket_place(item.item.id, placed, evaporated) {
            BucketPlace::Drop => Self::drop_item(ctx, item),
            BucketPlace::Empty { mob } => {
                if let Some(mob) = mob {
                    Self::spawn_bucket_mob(ctx, item, front, mob);
                }
                if !evaporated {
                    ctx.world.play_sound(
                        bucket_empty_sound(item.item.id),
                        SoundCategory::Blocks,
                        &front.to_f64(),
                    );
                }
                Self::consume_with_remainder(
                    ctx,
                    dispenser,
                    item,
                    ItemStack::new(1, &Item::BUCKET),
                );
                Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
            }
        }
    }

    fn spawn_bucket_mob(
        ctx: &DispenseContext<'_>,
        item: &ItemStack,
        pos: BlockPos,
        mob: BucketMob,
    ) {
        let spawned = from_type(
            bucket_mob_entity(mob),
            bucket_mob_spawn(pos),
            ctx.world,
            Uuid::new_v4(),
        );
        prepare_egg_mob(item, &spawned, ctx.world, None);
        if let Some(axolotl) = spawned.cast_any().downcast_ref::<AxolotlEntity>() {
            axolotl.set_from_bucket(true);
        }
        if let Some(mob) = spawned.get_mob() {
            mob.get_mob_entity()
                .persistence_required
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        ctx.world.spawn_entity(spawned);
    }

    fn dispense_bone_meal(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let target = Self::target_position(ctx);
        let block = ctx.world.get_block(&target);
        let state_id = ctx.world.get_block_state_id(&target);
        if ctx
            .world
            .block_registry
            .bone_meal(block, ctx.world, &target, state_id)
        {
            item.decrement(1);
            ctx.world
                .sync_world_event(WorldEvent::ParticlesAndSoundPlantGrowth, target, 15);
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
        }
    }

    fn dispense_experience_bottle(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let _ = item.split(1);
        let entity = Entity::new(
            ctx.world.clone(),
            Self::projectile_spawn_position(ctx),
            &EntityType::EXPERIENCE_BOTTLE,
        );
        let bottle = ThrownExperienceBottle::new(entity);
        Self::launch_thrown(
            ctx,
            &bottle.thrown,
            Self::DEFAULT_PROJECTILE_POWER,
            Self::DEFAULT_PROJECTILE_UNCERTAINTY,
        );
        ctx.world.play_sound(
            Sound::EntityExperienceBottleThrow,
            SoundCategory::Neutral,
            &bottle.thrown.entity.pos.load(),
        );
        Self::finish_projectile_launch(
            ctx,
            Arc::new(bottle),
            WorldEvent::SoundDispenserProjectileLaunch,
        );
    }

    fn dispense_flint_and_steel(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let front = Self::target_position(ctx);
        let front_block = ctx.world.get_block(&front);

        let ignited = if front_block == &Block::TNT {
            TNTBlock::prime(ctx.world, &front)
        } else {
            Ignition::ignite_block(
                |world: Arc<World>, pos: BlockPos, new_state_id: BlockStateId| {
                    world.set_block_state(&pos, new_state_id, BlockFlags::NOTIFY_ALL);
                },
                ctx.world,
                front,
                front,
                front_block,
            )
        };

        if ignited {
            // `damage_item` already consumes the tool from the stack when it breaks.
            let _ = item.damage_item(1);
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
        }
    }

    fn dispense_honeycomb(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        let front = Self::target_position(ctx);
        let front_block = ctx.world.get_block(&front);

        if try_wax_block(ctx.world, front, front_block) {
            item.decrement(1);
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
        }
    }

    fn dispense_minecart(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        fn rail_is_ascending(world: &Arc<World>, pos: &BlockPos) -> Option<bool> {
            let (block, state_id) = world.get_block_and_state_id(pos);
            if !block.has_tag(&tag::Block::MINECRAFT_RAILS) {
                return None;
            }
            Some(if PoweredRailLikeProperties::handles_block_id(block.id) {
                PoweredRailLikeProperties::from_state_id(state_id)
                    .shape
                    .is_ascending()
            } else {
                RailLikeProperties::from_state_id(state_id)
                    .shape
                    .is_ascending()
            })
        }

        let target = Self::target_position(ctx);
        let height = if let Some(ascending) = rail_is_ascending(ctx.world, &target) {
            if ascending { 0.6 } else { 0.1 }
        } else if ctx.world.get_block_state(&target).is_air()
            && let Some(ascending) = rail_is_ascending(ctx.world, &target.down())
        {
            if ascending && ctx.facing != Facing::Down {
                -0.4
            } else {
                -0.9
            }
        } else {
            return false;
        };

        let entity_type = MinecartItem::item_to_entity(item.item);
        let _ = item.split(1);

        let normal = to_normal(ctx.facing);
        let center = ctx.position.to_centered_f64();
        let spawn_pos = Vector3::new(
            normal.x.mul_add(1.125, center.x),
            f64::from(ctx.position.0.y) + normal.y + height,
            normal.z.mul_add(1.125, center.z),
        );

        let entity = Entity::new(ctx.world.clone(), spawn_pos, entity_type);
        ctx.world
            .spawn_entity(Arc::new(MinecartEntity::new(entity)));

        ctx.world
            .sync_world_event(WorldEvent::SoundDispenserDispense, *ctx.position, 0);
        true
    }

    fn dispense_shears(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        if Self::shear_pumpkin(ctx)
            || Self::shear_beehive(ctx)
            || Self::shear_entity_in_front(ctx)
        {
            // `damage_item` already consumes the tool from the stack when it breaks.
            let _ = item.damage_item(1);
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
        }
    }

    fn shear_pumpkin(ctx: &DispenseContext<'_>) -> bool {
        let target = Self::target_position(ctx);
        let block = ctx.world.get_block(&target);
        if block != &Block::PUMPKIN {
            return false;
        }

        ctx.world.set_block_state(
            &target,
            Block::CARVED_PUMPKIN.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        ctx.world.play_sound(
            Sound::BlockPumpkinCarve,
            SoundCategory::Blocks,
            &target.to_f64(),
        );
        Self::drop_at(
            ctx.world,
            target.to_centered_f64(),
            ItemStack::new(4, &Item::PUMPKIN_SEEDS),
        );
        true
    }

    fn shear_beehive(ctx: &DispenseContext<'_>) -> bool {
        const FULL_HONEY_LEVEL: u8 = 5;
        const HARVESTED_HONEYCOMBS: u8 = 3;

        let target = Self::target_position(ctx);
        let (block, state_id) = ctx.world.get_block_and_state_id(&target);
        if !block.has_tag(&tag::Block::MINECRAFT_BEEHIVES) {
            return false;
        }

        let mut props = BeeNestLikeProperties::from_state_id(state_id);
        if props.honey_level < FULL_HONEY_LEVEL {
            return false;
        }

        ctx.world
            .play_block_sound(Sound::BlockBeehiveShear, SoundCategory::Blocks, target);
        Self::drop_at(
            ctx.world,
            target.to_centered_f64(),
            ItemStack::new(HARVESTED_HONEYCOMBS, &Item::HONEYCOMB),
        );

        props.honey_level = 0;
        ctx.world
            .set_block_state(&target, props.to_state_id(block), BlockFlags::NOTIFY_ALL);

        true
    }

    fn shear_entity_in_front(ctx: &DispenseContext<'_>) -> bool {
        let target_box = BoundingBox::from_block(&Self::target_position(ctx));

        for entity in ctx.world.get_entities_at_box(&target_box) {
            if Self::try_shear_entity(ctx, &entity) {
                return true;
            }
        }

        false
    }

    fn try_shear_entity(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let base = entity.get_entity();
        if !base.is_alive() {
            return false;
        }

        let (baby, sheared, has_pumpkin) = Self::shear_flags(entity.as_ref());
        let effect = shear_effect(
            base.entity_type.resource_name,
            &ShearState {
                baby,
                sheared,
                has_pumpkin,
                leashed: base.is_leashed(),
            },
        );
        if effect == ShearEffect::None || Self::shear_cancelled(ctx, entity) {
            return false;
        }

        match effect {
            ShearEffect::Wool => Self::shear_sheep(ctx, entity),
            ShearEffect::Mushrooms => Self::shear_mooshroom(ctx, entity),
            ShearEffect::Pumpkin => Self::shear_snow_golem(ctx, entity),
            ShearEffect::BreakKnot => Self::shear_leash_knot(ctx, entity),
            ShearEffect::CutLeash => Self::cut_leash(ctx, entity),
            ShearEffect::None => false,
        }
    }

    fn shear_flags(entity: &dyn EntityBase) -> (bool, bool, bool) {
        if let Some(sheep) = entity.cast_any().downcast_ref::<SheepEntity>() {
            return (sheep.is_baby(), sheep.is_sheared(), false);
        }
        if let Some(mooshroom) = entity.cast_any().downcast_ref::<MooshroomEntity>() {
            return (mooshroom.is_baby(), false, false);
        }
        if let Some(golem) = entity.cast_any().downcast_ref::<SnowGolemEntity>() {
            return (false, false, golem.has_pumpkin());
        }
        (false, false, false)
    }

    fn shear_cancelled(ctx: &DispenseContext<'_>, target: &Arc<dyn EntityBase>) -> bool {
        let Some(server) = ctx.world.server.upgrade() else {
            return false;
        };
        let mut event =
            crate::plugin::api::events::block::block_shear_entity::BlockShearEntityEvent::new(
                *ctx.position,
                ctx.world.clone(),
                target.clone(),
                ItemStack::new(1, &Item::SHEARS),
            );
        server.plugin_manager.fire_blocking(&server, &mut event);
        event.cancelled
    }

    fn shear_sheep(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let Some(sheep) = entity.cast_any().downcast_ref::<SheepEntity>() else {
            return false;
        };
        let position = entity.get_entity().pos.load();
        sheep.set_sheared(true);
        ctx.world
            .play_sound(Sound::EntitySheepShear, SoundCategory::Blocks, &position);
        let count = rng().random_range(1..=3);
        Self::drop_at(
            ctx.world,
            position,
            ItemStack::new(count, wool_of_color(sheep.get_color())),
        );
        true
    }

    fn shear_mooshroom(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let Some(mooshroom) = entity.cast_any().downcast_ref::<MooshroomEntity>() else {
            return false;
        };
        let base = entity.get_entity();
        let position = base.pos.load();
        let age = mooshroom.get_age();
        let yaw = base.yaw.load();
        let pitch = base.pitch.load();
        let mushroom = if mooshroom.get_variant() == MooshroomVariant::Brown {
            &Item::BROWN_MUSHROOM
        } else {
            &Item::RED_MUSHROOM
        };

        ctx.world.play_sound(
            Sound::EntityMooshroomShear,
            SoundCategory::Blocks,
            &position,
        );
        for _ in 0..5 {
            Self::drop_at(ctx.world, position, ItemStack::new(1, mushroom));
        }
        ctx.world.spawn_particle(
            position + Vector3::new(0.0, 0.5, 0.0),
            Vector3::new(0.5, 0.5, 0.5),
            0.0,
            1,
            Particle::Explosion,
        );

        base.remove();
        let cow = from_type(&EntityType::COW, position, ctx.world, Uuid::new_v4());
        if let Some(ageable) = cow.get_mob().and_then(|mob| mob.as_ageable()) {
            ageable.set_age(age);
        }
        cow.get_entity().set_rotation(yaw, pitch);
        ctx.world.spawn_entity(cow);
        true
    }

    fn shear_snow_golem(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let Some(golem) = entity.cast_any().downcast_ref::<SnowGolemEntity>() else {
            return false;
        };
        let position = entity.get_entity().pos.load();
        golem.set_has_pumpkin(false);
        ctx.world.play_sound(
            Sound::EntitySnowGolemShear,
            SoundCategory::Blocks,
            &position,
        );
        Self::drop_at(
            ctx.world,
            position,
            ItemStack::new(1, &Item::CARVED_PUMPKIN),
        );
        true
    }

    fn shear_leash_knot(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let Some(knot) = entity.cast_any().downcast_ref::<LeashKnotEntity>() else {
            return false;
        };
        let knot_id = entity.get_entity().entity_id;
        let pos = entity.get_entity().pos.load();
        let search = BoundingBox::new_from_pos(
            pos.x,
            pos.y,
            pos.z,
            &EntityDimensions {
                width: 32.0,
                height: 32.0,
                eye_height: 16.0,
            },
        );
        let mut released = false;
        for other in ctx.world.get_entities_at_box(&search) {
            let other_entity = other.get_entity();
            let attached = other_entity
                .leashed_to
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|holder| holder.get_entity().entity_id == knot_id);
            if attached && Self::cut_leash(ctx, &other) {
                released = true;
            }
        }
        if released {
            knot.get_entity().remove();
        }
        released
    }

    fn cut_leash(ctx: &DispenseContext<'_>, entity: &Arc<dyn EntityBase>) -> bool {
        let base = entity.get_entity();
        if !base.is_leashed() {
            return false;
        }
        let position = base.pos.load();
        base.unleash();
        if base.is_leashed() {
            return false;
        }
        Self::drop_at(ctx.world, position, ItemStack::new(1, &Item::LEAD));
        ctx.world
            .play_sound(Sound::ItemLeadUntied, SoundCategory::Neutral, &position);
        true
    }

    fn dispense_glass_bottle(
        ctx: &DispenseContext<'_>,
        dispenser: &DispenserBlockEntity,
        item: &mut ItemStack,
    ) -> bool {
        const FULL_HONEY_LEVEL: u8 = 5;

        let target = Self::target_position(ctx);
        let (block, state_id) = ctx.world.get_block_and_state_id(&target);

        if block.has_tag(&tag::Block::MINECRAFT_BEEHIVES) {
            let mut props = BeeNestLikeProperties::from_state_id(state_id);
            if props.honey_level < FULL_HONEY_LEVEL {
                return false;
            }

            props.honey_level = 0;
            ctx.world
                .set_block_state(&target, props.to_state_id(block), BlockFlags::NOTIFY_ALL);
            Self::consume_with_remainder(
                ctx,
                dispenser,
                item,
                ItemStack::new(1, &Item::HONEY_BOTTLE),
            );
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
            return true;
        }

        let fluid = ctx.world.get_fluid(&target);
        if fluid.id == Fluid::WATER.id || fluid.id == Fluid::FLOWING_WATER.id {
            Self::consume_with_remainder(ctx, dispenser, item, water_bottle());
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
            return true;
        }

        false
    }

    fn dispense_water_bottle(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        let target = Self::target_position(ctx);
        if !ctx
            .world
            .get_block(&target)
            .has_tag(&tag::Block::MINECRAFT_CONVERTIBLE_TO_MUD)
        {
            return false;
        }

        ctx.world.spawn_particle(
            target.to_centered_f64(),
            Vector3::new(0.5, 0.5, 0.5),
            1.0,
            5,
            Particle::Splash,
        );
        ctx.world
            .play_block_sound(Sound::ItemBottleEmpty, SoundCategory::Blocks, target);
        ctx.world
            .set_block_state(&target, Block::MUD.default_state.id, BlockFlags::NOTIFY_ALL);

        *item = ItemStack::new(1, &Item::GLASS_BOTTLE);
        Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        true
    }

    fn dispense_glowstone(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> Option<bool> {
        const MAX_CHARGES: u8 = 4;

        let target = Self::target_position(ctx);
        let (block, state_id) = ctx.world.get_block_and_state_id(&target);
        if block != &Block::RESPAWN_ANCHOR {
            return None;
        }

        let mut props = RespawnAnchorLikeProperties::from_state_id(state_id);
        if props.charges >= MAX_CHARGES {
            return Some(false);
        }

        props.charges += 1;
        let _ = item.split(1);
        ctx.world
            .set_block_state(&target, props.to_state_id(block), BlockFlags::NOTIFY_ALL);
        ctx.world.play_block_sound(
            Sound::BlockRespawnAnchorCharge,
            SoundCategory::Blocks,
            target,
        );

        Some(true)
    }

    fn dispense_mob_head(ctx: &DispenseContext<'_>, item: &mut ItemStack, block: &'static Block) {
        let target = Self::target_position(ctx);

        let summons_mob = ctx.world.get_block_state(&target).is_air()
            && if block == &Block::WITHER_SKELETON_SKULL {
                find_wither_pattern(ctx.world, &target).is_some()
            } else {
                find_golem_pattern(ctx.world, &target).is_some()
            };

        if summons_mob {
            let state_id = if block == &Block::WITHER_SKELETON_SKULL {
                let mut props = SkeletonSkullLikeProperties::default(block);
                props.rotation = to_rotation16(ctx.facing);
                props.to_state_id(block)
            } else {
                block.default_state.id
            };

            let _ = item.split(1);
            ctx.world
                .set_block_state(&target, state_id, BlockFlags::NOTIFY_ALL);
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else if Self::dispense_equipment(ctx, item) {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);
        } else {
            Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserFail);
        }
    }

    fn dispense_shulker_box(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        let Some(block) = Block::from_item_id(item.item.id) else {
            return false;
        };

        let target = Self::target_position(ctx);
        if !ctx.world.get_block_state(&target).replaceable() {
            return false;
        }

        let mut props = EndRodLikeProperties::default(block);
        props.facing = if ctx.world.get_block_state(&target.down()).is_air() {
            ctx.facing
        } else {
            Facing::Up
        };

        let placed = item.split(1);
        ctx.world
            .set_block_state(&target, props.to_state_id(block), BlockFlags::NOTIFY_ALL);
        if let Some(block_entity) = ctx.world.get_block_entity(&target) {
            block_entity.apply_item_components(&placed);
        }
        Self::play_dispense_effects(ctx, WorldEvent::SoundDispenserDispense);

        true
    }

    fn dispense_equipment(ctx: &DispenseContext<'_>, item: &mut ItemStack) -> bool {
        let (slot, allowed_entities, equip_sound) = {
            let Some(equippable) = item.get_data_component::<EquippableImpl>() else {
                return false;
            };
            if !equippable.dispensable {
                return false;
            }
            (
                equippable.slot,
                equippable.allowed_entities.clone(),
                equippable.equip_sound.clone(),
            )
        };

        let target_box = BoundingBox::from_block(&Self::target_position(ctx));
        let players = ctx
            .world
            .get_players_at_box(&target_box)
            .into_iter()
            .map(|player| player as Arc<dyn EntityBase>);

        for entity in ctx
            .world
            .get_entities_at_box(&target_box)
            .into_iter()
            .chain(players)
        {
            let Some(living) = entity.get_living_entity() else {
                continue;
            };
            if !living.is_part_of_game()
                || !is_allowed_entity(allowed_entities.as_ref(), entity.get_entity().entity_type)
            {
                continue;
            }

            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !equipment.get(slot).is_empty() {
                continue;
            }

            let stack = item.split(1);
            equipment.put(slot, stack.clone());
            drop(equipment);

            living.send_equipment_changes(&[(slot.clone(), stack)]);
            ctx.world.play_sound_event(
                &equip_sound,
                SoundCategory::Blocks,
                &entity.get_entity().pos.load(),
            );

            return true;
        }

        false
    }

    fn consume_with_remainder(
        ctx: &DispenseContext<'_>,
        dispenser: &DispenserBlockEntity,
        item: &mut ItemStack,
        remainder: ItemStack,
    ) {
        item.decrement(1);
        if item.is_empty() {
            *item = remainder;
        } else if let Some(rest) = Self::add_to_first_free_slot(dispenser, remainder) {
            Self::eject_item(ctx, rest);
        }
    }

    fn drop_at(world: &Arc<World>, position: Vector3<f64>, stack: ItemStack) {
        let entity = Entity::new(world.clone(), position, &EntityType::ITEM);
        world.spawn_entity(Arc::new(ItemEntity::new(entity, stack)));
    }

    fn drop_item(ctx: &DispenseContext<'_>, item: &mut ItemStack) {
        Self::drop_item_with_sound(ctx, item, WorldEvent::SoundDispenserDispense);
    }

    fn drop_item_with_sound(ctx: &DispenseContext<'_>, item: &mut ItemStack, sound: WorldEvent) {
        let drop_item = item.split(1);
        Self::eject_item(ctx, drop_item);
        Self::play_dispense_effects(ctx, sound);
    }

    fn eject_item(ctx: &DispenseContext<'_>, stack: ItemStack) {
        let facing = to_normal(ctx.facing);
        let mut position = ctx.position.to_centered_f64().add(&(facing * 0.7));

        position.y -= match ctx.facing {
            Facing::Up | Facing::Down => 0.125,
            _ => 0.15625,
        };

        let entity = Entity::new(ctx.world.clone(), position, &EntityType::ITEM);
        let rd = rng().random::<f64>().mul_add(0.1, 0.2);

        let velocity = Vector3::new(
            triangle(&mut rng(), facing.x * rd, 0.017_227_5 * 6.),
            triangle(&mut rng(), 0.2, 0.017_227_5 * 6.),
            triangle(&mut rng(), facing.z * rd, 0.017_227_5 * 6.),
        );

        let item_entity = Arc::new(ItemEntity::new_with_velocity(entity, stack, velocity, 40));
        ctx.world.spawn_entity(item_entity);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BucketMob, BucketPlace, DirectItemUse, ShearEffect, ShearState, bucket_empty_sound,
        bucket_mob_entity, bucket_mob_spawn, bucket_place, direct_item_use,
        experience_bottle_amount, shear_effect,
    };
    use pumpkin_data::entity::EntityType;
    use pumpkin_data::item::Item;
    use pumpkin_data::sound::Sound;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector3::Vector3;

    #[test]
    fn bone_meal_and_experience_bottles_are_used_not_dropped() {
        assert_eq!(
            direct_item_use(Item::BONE_MEAL.id),
            Some(DirectItemUse::BoneMeal)
        );
        assert_eq!(
            direct_item_use(Item::EXPERIENCE_BOTTLE.id),
            Some(DirectItemUse::ExperienceBottle)
        );
        assert_eq!(direct_item_use(Item::STICK.id), None);
        assert_eq!(direct_item_use(Item::ARROW.id), None);
    }

    #[test]
    fn experience_bottle_amount_is_three_through_eleven() {
        let amounts: Vec<u32> = (0..EXPERIENCE_SPAN_CHECK)
            .map(experience_bottle_amount)
            .collect();
        assert!(amounts.iter().all(|amount| (3..=11).contains(amount)));
        assert_eq!(experience_bottle_amount(0), 3);
        assert_eq!(experience_bottle_amount(8), 11);
        assert_eq!(experience_bottle_amount(9), 3);
    }

    const EXPERIENCE_SPAN_CHECK: u32 = 18;

    #[test]
    fn placed_buckets_become_empty_and_keep_aquatic_mobs() {
        let fluids = [
            Item::WATER_BUCKET.id,
            Item::LAVA_BUCKET.id,
            Item::POWDER_SNOW_BUCKET.id,
        ];
        for item_id in fluids {
            assert_eq!(
                bucket_place(item_id, true, false),
                BucketPlace::Empty { mob: None }
            );
            assert_eq!(bucket_place(item_id, false, false), BucketPlace::Drop);
        }

        let mobs = [
            (
                Item::AXOLOTL_BUCKET.id,
                BucketMob::Axolotl,
                &EntityType::AXOLOTL,
            ),
            (Item::COD_BUCKET.id, BucketMob::Cod, &EntityType::COD),
            (
                Item::SALMON_BUCKET.id,
                BucketMob::Salmon,
                &EntityType::SALMON,
            ),
            (
                Item::PUFFERFISH_BUCKET.id,
                BucketMob::Pufferfish,
                &EntityType::PUFFERFISH,
            ),
            (
                Item::TROPICAL_FISH_BUCKET.id,
                BucketMob::TropicalFish,
                &EntityType::TROPICAL_FISH,
            ),
            (
                Item::TADPOLE_BUCKET.id,
                BucketMob::Tadpole,
                &EntityType::TADPOLE,
            ),
        ];
        for (item_id, mob, entity_type) in mobs {
            assert_eq!(
                bucket_place(item_id, true, false),
                BucketPlace::Empty { mob: Some(mob) }
            );
            assert_eq!(
                bucket_place(item_id, true, true),
                BucketPlace::Empty { mob: None }
            );
            assert_eq!(bucket_place(item_id, false, false), BucketPlace::Drop);
            assert_eq!(bucket_mob_entity(mob).id, entity_type.id);
        }
    }

    #[test]
    fn bucket_empty_sounds_match_contents() {
        assert_eq!(
            bucket_empty_sound(Item::WATER_BUCKET.id),
            Sound::ItemBucketEmpty
        );
        assert_eq!(
            bucket_empty_sound(Item::LAVA_BUCKET.id),
            Sound::ItemBucketEmptyLava
        );
        assert_eq!(
            bucket_empty_sound(Item::POWDER_SNOW_BUCKET.id),
            Sound::ItemBucketEmptyPowderSnow
        );
        assert_eq!(
            bucket_empty_sound(Item::AXOLOTL_BUCKET.id),
            Sound::ItemBucketEmptyAxolotl
        );
        assert_eq!(
            bucket_empty_sound(Item::TADPOLE_BUCKET.id),
            Sound::ItemBucketEmptyTadpole
        );
        assert_eq!(
            bucket_empty_sound(Item::COD_BUCKET.id),
            Sound::ItemBucketEmptyFish
        );
        assert_eq!(
            bucket_empty_sound(Item::SALMON_BUCKET.id),
            Sound::ItemBucketEmptyFish
        );
        assert_eq!(
            bucket_empty_sound(Item::PUFFERFISH_BUCKET.id),
            Sound::ItemBucketEmptyFish
        );
        assert_eq!(
            bucket_empty_sound(Item::TROPICAL_FISH_BUCKET.id),
            Sound::ItemBucketEmptyFish
        );
    }

    #[test]
    fn bucket_mob_spawns_on_the_placed_fluid() {
        let pos = BlockPos::new(3, 64, -2);
        assert_eq!(bucket_mob_spawn(pos), Vector3::new(3.5, 64.0, -1.5));
    }

    fn shear(
        resource_name: &str,
        baby: bool,
        sheared: bool,
        has_pumpkin: bool,
        leashed: bool,
    ) -> ShearEffect {
        shear_effect(
            resource_name,
            &ShearState {
                baby,
                sheared,
                has_pumpkin,
                leashed,
            },
        )
    }

    #[test]
    fn shears_cover_sheep_mooshrooms_snow_golems_and_leads() {
        assert_eq!(shear("sheep", false, false, false, true), ShearEffect::Wool);
        assert_eq!(shear("sheep", true, false, false, false), ShearEffect::None);
        assert_eq!(
            shear("sheep", true, false, false, true),
            ShearEffect::CutLeash
        );
        assert_eq!(shear("sheep", false, true, false, false), ShearEffect::None);
        assert_eq!(
            shear("sheep", false, true, false, true),
            ShearEffect::CutLeash
        );
        assert_eq!(
            shear("mooshroom", false, false, false, false),
            ShearEffect::Mushrooms
        );
        assert_eq!(
            shear("mooshroom", true, false, false, false),
            ShearEffect::None
        );
        assert_eq!(
            shear("snow_golem", false, false, true, false),
            ShearEffect::Pumpkin
        );
        assert_eq!(
            shear("snow_golem", false, false, false, false),
            ShearEffect::None
        );
        assert_eq!(
            shear("leash_knot", false, false, false, false),
            ShearEffect::BreakKnot
        );
        assert_eq!(
            shear("cow", false, false, false, true),
            ShearEffect::CutLeash
        );
        assert_eq!(shear("cow", false, false, false, false), ShearEffect::None);
    }
}
