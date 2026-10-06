use std::io::Write;

use pumpkin_data::packet::clientbound::play::LEVEL_PARTICLES;
use pumpkin_macros::java_packet;
use pumpkin_util::{math::vector3::Vector3, version::JavaMinecraftVersion};

use crate::{
    ClientPacket, ServerPacket, VarInt,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};

/// Spawns a cluster of particles at a specific location.
///
/// This is the most versatile visual packet in the protocol. It allows for
/// precise control over particle density, spread, and speed. It can also
/// carry extra data for complex particles like redstone dust (color) or
/// block/item breaking (textures).
#[java_packet(LEVEL_PARTICLES)]
#[derive(Clone, Debug, PartialEq)]
pub struct CParticle<'a> {
    /// If true, the particle renders even if the client's "Particles"
    /// setting is set to "Minimal".
    pub force_spawn: bool,
    /// If true, the distance at which particles are visible is significantly
    /// increased (from 256 to 65536 blocks). Often used for massive events.
    pub important: bool,
    /// The absolute center position of the particle cluster.
    pub position: Vector3<f64>,
    /// The maximum distance from the center that particles can spawn.
    pub offset: Vector3<f32>,
    /// The velocity or "spread" speed of the particles.
    pub max_speed: f32,
    /// The total number of particles to spawn in this cluster.
    pub particle_count: i32,
    /// The ID of the particle type (e.g., `minecraft:flame`).
    pub particle_id: VarInt,
    /// Extra data required by specific particles (e.g., block states for
    /// `block` particles or RGB values for `dust`).
    pub data: &'a [u8],
}

/// The payload that follows the particle ID for particle types whose vanilla
/// `ParticleOptions` is not a plain `SimpleParticleType`.
///
/// The particle ID is taken from [`Self::particle`] rather than passed
/// alongside, so a payload can never be paired with the wrong particle.
/// Particles that take no options are spawned through [`CParticle`] directly
/// with an empty `data`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParticleOptions {
    /// `minecraft:trail`, vanilla's `TrailParticleOption`.
    Trail {
        /// The position the trail travels towards.
        target: Vector3<f64>,
        /// The trail color, encoded as `0xRRGGBB`. The top bits are ignored.
        color: i32,
        /// Life time in ticks.
        duration: i32,
    },
    /// `minecraft:block_crumble`, vanilla's `BlockParticleOption`.
    BlockCrumble {
        /// The global block state palette ID of the crumbling block.
        state: i32,
    },
    /// `minecraft:effect`, vanilla's `ColorParticleOption`.
    Effect {
        /// The particle color, encoded as `0xRRGGBB`. The top bits are ignored.
        color: i32,
        /// Particle potency/scale.
        power: f32,
    },
}

impl ParticleOptions {
    /// The particle this payload belongs to.
    #[must_use]
    pub const fn particle(&self) -> pumpkin_data::particle::Particle {
        match self {
            Self::Trail { .. } => pumpkin_data::particle::Particle::Trail,
            Self::BlockCrumble { .. } => pumpkin_data::particle::Particle::BlockCrumble,
            Self::Effect { .. } => pumpkin_data::particle::Particle::Effect,
        }
    }

    /// Whether `version` still has this particle under an ID of its own.
    ///
    /// On older versions the particle may be remapped onto an unrelated one
    /// that reads a different payload — `trail` folds onto `entity_effect`,
    /// for instance — so no byte string we could write would be read correctly
    /// and the packet has to be dropped instead.
    #[must_use]
    pub fn is_supported_by(&self, version: JavaMinecraftVersion) -> bool {
        match self {
            // `trail` was added in 1.21.2 with the pale garden
            Self::Trail { .. } => version >= JavaMinecraftVersion::V_1_21_2,
            Self::BlockCrumble { .. } | Self::Effect { .. } => true,
        }
    }

    /// Writes the payload that follows the particle ID.
    pub fn write(&self, write: impl Write) -> Result<(), WritingError> {
        let mut write = write;
        match *self {
            Self::Trail {
                target,
                color,
                duration,
            } => {
                write.write_f64_be(target.x)?;
                write.write_f64_be(target.y)?;
                write.write_f64_be(target.z)?;
                write.write_i32_be(color)?;
                write.write_var_int(&VarInt(duration))
            }
            Self::BlockCrumble { state } => write.write_var_int(&VarInt(state)),
            Self::Effect { color, power } => {
                write.write_i32_be(color)?;
                write.write_f32_be(power)
            }
        }
    }

    /// The payload to send to a client on `version`, or `None` when that client
    /// cannot read this particle and the packet must be dropped instead.
    ///
    /// See [`Self::is_supported_by`] for when that happens. `None` also covers a
    /// write failure, which a `Vec` sink cannot actually produce.
    #[must_use]
    pub fn encode_for(&self, version: JavaMinecraftVersion) -> Option<Vec<u8>> {
        if !self.is_supported_by(version) {
            return None;
        }
        let mut payload = Vec::new();
        self.write(&mut payload).ok()?;
        Some(payload)
    }
}

impl<'a> CParticle<'a> {
    #[expect(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        force_spawn: bool,
        important: bool,
        position: Vector3<f64>,
        offset: Vector3<f32>,
        max_speed: f32,
        particle_count: i32,
        particle_id: VarInt,
        data: &'a [u8],
    ) -> Self {
        Self {
            force_spawn,
            important,
            position,
            offset,
            max_speed,
            particle_count,
            particle_id,
            data,
        }
    }
}

#[must_use]
pub const fn particle_name_for_v1_7(particle: pumpkin_data::particle::Particle) -> &'static str {
    use pumpkin_data::particle::Particle::{
        AngryVillager, Block, BlockCrumble, BlockMarker, Bubble, BubbleColumnUp, BubblePop,
        CampfireSignalSmoke, Cloud, Composter, Crit, DamageIndicator, DragonBreath,
        DrippingDripstoneLava, DrippingDripstoneWater, DrippingLava, DrippingWater, Dust,
        DustColorTransition, DustPillar, DustPlume, Effect, Enchant, EnchantedHit, EntityEffect,
        Explosion, ExplosionEmitter, FallingDust, Firework, Fishing, Flame, HappyVillager, Heart,
        InstantEffect, Item, ItemSlime, ItemSnowball, LargeSmoke, Lava, Mycelium, Note, Poof,
        Portal, Rain, ReversePortal, SmallFlame, Snowflake, SoulFireFlame, Splash, SweepAttack,
        TotemOfUndying, Underwater, Witch,
    };
    match particle {
        ExplosionEmitter => "hugeexplosion",
        Explosion => "largeexplode",
        Poof => "explode",
        Firework => "fireworksSpark",
        Bubble | BubblePop | BubbleColumnUp => "bubble",
        Splash => "splash",
        Fishing => "wake",
        Underwater => "suspended",
        Crit | DamageIndicator | SweepAttack => "crit",
        EnchantedHit => "magicCrit",
        LargeSmoke | CampfireSignalSmoke => "largesmoke",
        InstantEffect => "spell",
        EntityEffect => "mobSpell",
        Effect => "mobSpellAmbient",
        Witch | TotemOfUndying | DragonBreath => "witchMagic",
        DrippingWater | DrippingDripstoneWater => "dripWater",
        DrippingLava | DrippingDripstoneLava => "dripLava",
        AngryVillager => "angryVillager",
        HappyVillager | Composter => "happyVillager",
        Mycelium => "townaura",
        Note => "note",
        Portal | ReversePortal => "portal",
        Enchant => "enchantmenttable",
        Flame | SmallFlame | SoulFireFlame => "flame",
        Lava => "lava",
        Cloud => "cloud",
        Dust | DustColorTransition | DustPillar | DustPlume => "reddust",
        ItemSnowball | Snowflake => "snowballpoof",
        ItemSlime => "slime",
        Heart => "heart",
        BlockMarker => "barrier",
        Rain => "droplet",
        Item => "iconcrack_",
        Block | BlockCrumble => "blockcrack_",
        FallingDust => "blockdust_",
        _ => "smoke",
    }
}

#[must_use]
pub fn particle_id_from_1_7_name(name: &str) -> i32 {
    match name {
        "explode" => pumpkin_data::particle::Particle::Poof as i32,
        "largeexplode" => pumpkin_data::particle::Particle::Explosion as i32,
        "hugeexplosion" => pumpkin_data::particle::Particle::ExplosionEmitter as i32,
        "fireworksSpark" => pumpkin_data::particle::Particle::Firework as i32,
        "bubble" => pumpkin_data::particle::Particle::Bubble as i32,
        "splash" => pumpkin_data::particle::Particle::Splash as i32,
        "wake" => pumpkin_data::particle::Particle::Fishing as i32,
        "suspended" | "depthsuspend" => pumpkin_data::particle::Particle::Underwater as i32,
        "crit" => pumpkin_data::particle::Particle::Crit as i32,
        "magicCrit" => pumpkin_data::particle::Particle::EnchantedHit as i32,
        "smoke" => pumpkin_data::particle::Particle::Smoke as i32,
        "largesmoke" => pumpkin_data::particle::Particle::LargeSmoke as i32,
        "spell" | "instantSpell" => pumpkin_data::particle::Particle::InstantEffect as i32,
        "mobSpell" => pumpkin_data::particle::Particle::EntityEffect as i32,
        "mobSpellAmbient" => pumpkin_data::particle::Particle::Effect as i32,
        "witchMagic" => pumpkin_data::particle::Particle::Witch as i32,
        "dripWater" => pumpkin_data::particle::Particle::DrippingWater as i32,
        "dripLava" => pumpkin_data::particle::Particle::DrippingLava as i32,
        "angryVillager" => pumpkin_data::particle::Particle::AngryVillager as i32,
        "happyVillager" => pumpkin_data::particle::Particle::HappyVillager as i32,
        "townaura" => pumpkin_data::particle::Particle::Mycelium as i32,
        "note" => pumpkin_data::particle::Particle::Note as i32,
        "portal" => pumpkin_data::particle::Particle::Portal as i32,
        "enchantmenttable" => pumpkin_data::particle::Particle::Enchant as i32,
        "flame" => pumpkin_data::particle::Particle::Flame as i32,
        "lava" => pumpkin_data::particle::Particle::Lava as i32,
        "cloud" => pumpkin_data::particle::Particle::Cloud as i32,
        "reddust" => pumpkin_data::particle::Particle::Dust as i32,
        "snowballpoof" | "snowshovel" => pumpkin_data::particle::Particle::ItemSnowball as i32,
        "slime" => pumpkin_data::particle::Particle::ItemSlime as i32,
        "heart" => pumpkin_data::particle::Particle::Heart as i32,
        "barrier" => pumpkin_data::particle::Particle::BlockMarker as i32,
        "droplet" => pumpkin_data::particle::Particle::Rain as i32,
        _ => pumpkin_data::particle::Particle::from_name(name).map_or(0, |p| p as i32),
    }
}

impl ClientPacket for CParticle<'_> {
    fn write_packet_data(
        &self,
        write: impl Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), WritingError> {
        let mut write = write;

        if *version <= JavaMinecraftVersion::V_1_7_6 {
            let name = pumpkin_data::particle::Particle::from_id(self.particle_id.0 as u16)
                .map_or("smoke", particle_name_for_v1_7);
            write.write_string_bounded(name, 64)?;
        } else if *version < JavaMinecraftVersion::V_1_20_5 {
            if *version >= JavaMinecraftVersion::V_1_19 {
                write.write_var_int(&self.particle_id)?;
            } else {
                write.write_i32_be(self.particle_id.0)?;
            }
        } else if *version >= JavaMinecraftVersion::V_26_3 {
            // The particle moved back to the front of the packet in 26.3
            write.write_var_int(&self.particle_id)?;
            write.write_slice(self.data)?;
        }

        if *version >= JavaMinecraftVersion::V_1_8 {
            write.write_bool(self.important)?;
        }
        if *version >= JavaMinecraftVersion::V_1_21_4 {
            write.write_bool(self.force_spawn)?;
        }

        if *version >= JavaMinecraftVersion::V_1_15 {
            write.write_f64_be(self.position.x)?;
            write.write_f64_be(self.position.y)?;
            write.write_f64_be(self.position.z)?;
        } else {
            write.write_f32_be(self.position.x as f32)?;
            write.write_f32_be(self.position.y as f32)?;
            write.write_f32_be(self.position.z as f32)?;
        }

        write.write_f32_be(self.offset.x)?;
        write.write_f32_be(self.offset.y)?;
        write.write_f32_be(self.offset.z)?;

        write.write_f32_be(self.max_speed)?;
        if *version >= JavaMinecraftVersion::V_26_3 {
            // Since 26.3 the speed is set per axis and the count is a var int, followed by the
            // randomization type, 0 being the default one.
            write.write_f32_be(self.max_speed)?;
            write.write_f32_be(self.max_speed)?;
            write.write_var_int(&VarInt(self.particle_count))?;
            write.write_var_int(&VarInt(0))?;
            return Ok(());
        }
        write.write_i32_be(self.particle_count)?;

        if *version >= JavaMinecraftVersion::V_1_20_5 {
            write.write_var_int(&self.particle_id)?;
        }
        write.write_slice(self.data)?;

        Ok(())
    }
}

impl<'a> ServerPacket<'a> for CParticle<'a> {
    fn read(bytebuf: &mut &'a [u8], version: &JavaMinecraftVersion) -> Result<Self, ReadingError> {
        let (particle_id, important, force_spawn) = if *version <= JavaMinecraftVersion::V_1_7_6 {
            let name = bytebuf.get_str_bounded_borrowed(64)?;
            let id = particle_id_from_1_7_name(name);
            (VarInt(id), false, false)
        } else if *version < JavaMinecraftVersion::V_1_20_5 {
            let id = if *version >= JavaMinecraftVersion::V_1_19 {
                bytebuf.get_var_int()?
            } else {
                VarInt(bytebuf.get_i32_be()?)
            };
            let important = bytebuf.get_bool()?;
            (id, important, false)
        } else {
            let important = bytebuf.get_bool()?;
            let force_spawn = if *version >= JavaMinecraftVersion::V_1_21_4 {
                bytebuf.get_bool()?
            } else {
                false
            };
            (VarInt(0), important, force_spawn)
        };

        let position = if *version >= JavaMinecraftVersion::V_1_15 {
            Vector3::new(
                bytebuf.get_f64_be()?,
                bytebuf.get_f64_be()?,
                bytebuf.get_f64_be()?,
            )
        } else {
            Vector3::new(
                f64::from(bytebuf.get_f32_be()?),
                f64::from(bytebuf.get_f32_be()?),
                f64::from(bytebuf.get_f32_be()?),
            )
        };

        let offset = Vector3::new(
            bytebuf.get_f32_be()?,
            bytebuf.get_f32_be()?,
            bytebuf.get_f32_be()?,
        );
        let max_speed = bytebuf.get_f32_be()?;
        let particle_count = bytebuf.get_i32_be()?;

        let (particle_id, data) = if *version >= JavaMinecraftVersion::V_1_20_5 {
            let id = bytebuf.get_var_int()?;
            let remaining = bytebuf.read_remaining_slice_borrowed(usize::MAX)?;
            (id, remaining)
        } else {
            let remaining = bytebuf.read_remaining_slice_borrowed(usize::MAX)?;
            (particle_id, remaining)
        };

        Ok(Self {
            force_spawn,
            important,
            position,
            offset,
            max_speed,
            particle_count,
            particle_id,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pumpkin_data::particle::Particle;
    use pumpkin_util::{math::vector3::Vector3, version::JavaMinecraftVersion};

    use crate::{ClientPacket, VarInt};

    use super::CParticle;

    #[test]
    fn particle_encoding_legacy_1_7_string() {
        use crate::ser::NetworkReadSliceExt;

        let packet = CParticle::new(
            false,
            false,
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(0.1, 0.2, 0.3),
            0.5,
            10,
            VarInt(Particle::ExplosionEmitter as i32),
            &[],
        );
        let mut bytes = Vec::new();
        packet
            .write_packet_data(&mut bytes, &JavaMinecraftVersion::V_1_7_6)
            .unwrap();

        let mut slice = bytes.as_slice();
        let name = slice.get_str_borrowed().unwrap();
        assert_eq!(name, "hugeexplosion");
    }

    #[test]
    fn particle_encoding_1_8_int_id() {
        use crate::ser::NetworkReadExt;

        let packet = CParticle::new(
            false,
            false,
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(0.1, 0.2, 0.3),
            0.5,
            10,
            VarInt(Particle::ExplosionEmitter as i32),
            &[],
        );
        let mut bytes = Vec::new();
        packet
            .write_packet_data(&mut bytes, &JavaMinecraftVersion::V_1_8)
            .unwrap();

        let mut slice = bytes.as_slice();
        let id = slice.get_i32_be().unwrap();
        assert_eq!(id, Particle::ExplosionEmitter as i32);
    }

    #[test]
    fn particle_encoding_1_19_varint_id() {
        let packet = CParticle::new(
            false,
            false,
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(0.1, 0.2, 0.3),
            0.5,
            10,
            VarInt(Particle::ExplosionEmitter as i32),
            &[],
        );
        let mut bytes = Vec::new();
        packet
            .write_packet_data(&mut bytes, &JavaMinecraftVersion::V_1_19)
            .unwrap();

        let mut cursor = Cursor::new(bytes);
        let id = VarInt::decode(&mut cursor).unwrap();
        assert_eq!(id, VarInt(Particle::ExplosionEmitter as i32));
    }

    #[test]
    fn trail_options_encode_target_color_duration() {
        use crate::ser::NetworkReadExt;

        let options = super::ParticleOptions::Trail {
            target: Vector3::new(1.5, 2.5, 3.5),
            color: 0x00AB_CDEF,
            duration: 40,
        };
        let mut bytes = Vec::new();
        options.write(&mut bytes).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.get_f64_be().unwrap(), 1.5);
        assert_eq!(cursor.get_f64_be().unwrap(), 2.5);
        assert_eq!(cursor.get_f64_be().unwrap(), 3.5);
        assert_eq!(cursor.get_i32_be().unwrap(), 0x00AB_CDEF);
        assert_eq!(cursor.get_var_int().unwrap(), VarInt(40));
        assert_eq!(cursor.position(), cursor.get_ref().len() as u64);
    }

    #[test]
    fn block_crumble_options_encode_state_varint() {
        use crate::ser::NetworkReadExt;

        let options = super::ParticleOptions::BlockCrumble { state: 8593 };
        let mut bytes = Vec::new();
        options.write(&mut bytes).unwrap();

        let mut cursor = Cursor::new(bytes);
        assert_eq!(cursor.get_var_int().unwrap(), VarInt(8593));
        assert_eq!(cursor.position(), cursor.get_ref().len() as u64);
    }

    #[test]
    fn trail_options_dropped_for_versions_without_the_particle() {
        let options = super::ParticleOptions::Trail {
            target: Vector3::new(0.0, 0.0, 0.0),
            color: 0,
            duration: 10,
        };
        assert!(options.encode_for(JavaMinecraftVersion::V_1_21).is_none());
        assert!(options.encode_for(JavaMinecraftVersion::V_1_21_2).is_some());
        assert!(options.encode_for(JavaMinecraftVersion::V_26_3).is_some());
    }
}
