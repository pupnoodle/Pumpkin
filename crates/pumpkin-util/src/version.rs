/// Represents a specific version of the Minecraft Java Edition protocol.
///
/// Each variant corresponds to a released client version and its associated
/// network protocol number. Ordering reflects chronological release order,
/// allowing version comparisons using standard comparison operators.
///
/// `Unknown` is used when a protocol number is not recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[allow(non_camel_case_types)]
pub enum JavaMinecraftVersion {
    V_1_0,
    V_1_1,
    V_1_2_5,
    V_1_3_2,
    V_1_4_7,
    V_1_5_2,
    V_1_6_4,
    /// 1.7.2: The Update That Changed The World.
    V_1_7_2,
    V_1_7_3,
    V_1_7_4,
    V_1_7_5,
    V_1_7_6,
    V_1_7_7,
    V_1_7_8,
    V_1_7_9,
    V_1_7_10,
    /// 1.8: The Bountiful Update.
    V_1_8,
    V_1_8_1,
    V_1_8_2,
    V_1_8_3,
    V_1_8_4,
    V_1_8_5,
    V_1_8_6,
    V_1_8_7,
    V_1_8_8,
    V_1_8_9,
    /// 1.9: The Combat Update.
    V_1_9,
    V_1_9_1,
    V_1_9_2,
    V_1_9_3,
    V_1_9_4,
    /// 1.10: The Frostburn Update.
    V_1_10,
    V_1_10_1,
    V_1_10_2,
    /// 1.11: The Exploration Update.
    V_1_11,
    V_1_11_1,
    V_1_11_2,
    /// 1.12: The World of Color Update.
    V_1_12,
    V_1_12_1,
    V_1_12_2,
    /// 1.13: Update Aquatic.
    V_1_13,
    V_1_13_1,
    V_1_13_2,
    /// 1.14: Village & Pillage.
    V_1_14,
    V_1_14_1,
    V_1_14_2,
    V_1_14_3,
    V_1_14_4,
    /// 1.15: Buzzy Bees.
    V_1_15,
    V_1_15_1,
    V_1_15_2,
    /// 1.16: Nether Update.
    V_1_16,
    V_1_16_1,
    V_1_16_2,
    V_1_16_3,
    V_1_16_4,
    V_1_16_5,
    /// 1.17: Caves & Cliffs: Part I.
    V_1_17,
    V_1_17_1,
    /// 1.18: Caves & Cliffs: Part II.
    V_1_18,
    V_1_18_1,
    V_1_18_2,
    /// 1.19: The Wild Update.
    V_1_19,
    V_1_19_1,
    V_1_19_2,
    V_1_19_3,
    V_1_19_4,
    /// 1.20: Trails & Tales.
    V_1_20,
    V_1_20_1,
    V_1_20_2,
    V_1_20_3,
    V_1_20_4,
    /// 1.20.5: Armored Paws.
    V_1_20_5,
    V_1_20_6,
    /// 1.21: Tricky Trials.
    V_1_21,
    V_1_21_1,
    V_1_21_2,
    V_1_21_3,
    V_1_21_4,
    /// 1.21.5: Bundles of Bravery.
    V_1_21_5,
    V_1_21_6,
    V_1_21_7,
    V_1_21_8,
    V_1_21_9,
    V_1_21_10,
    V_1_21_11,
    //  26.1: Tiny Takeover
    V_26_1,
    V_26_1_1,
    V_26_1_2,
    // 26.2: Chaos Cubed
    V_26_2,
    /// 26.3: Wilderness Bound
    V_26_3,
    /// Fallback for unrecognized protocol versions.
    Unknown,
}

impl JavaMinecraftVersion {
    /// Returns the network protocol number for this version.
    ///
    /// Returns `-1` for [`JavaMinecraftVersion::Unknown`].
    #[must_use]
    pub const fn protocol_version(&self) -> i32 {
        match self {
            Self::V_1_0 => 22,
            Self::V_1_1 => 23,
            Self::V_1_2_5 => 29,
            Self::V_1_3_2 => 39,
            Self::V_1_4_7 => 51,
            Self::V_1_5_2 => 61,
            Self::V_1_6_4 => 78,
            Self::V_1_7_2 | Self::V_1_7_3 | Self::V_1_7_4 | Self::V_1_7_5 => 4,
            Self::V_1_7_6 | Self::V_1_7_7 | Self::V_1_7_8 | Self::V_1_7_9 | Self::V_1_7_10 => 5,
            Self::V_1_8
            | Self::V_1_8_1
            | Self::V_1_8_2
            | Self::V_1_8_3
            | Self::V_1_8_4
            | Self::V_1_8_5
            | Self::V_1_8_6
            | Self::V_1_8_7
            | Self::V_1_8_8
            | Self::V_1_8_9 => 47,
            Self::V_1_9 => 107,
            Self::V_1_9_1 => 108,
            Self::V_1_9_2 => 109,
            Self::V_1_9_3 | Self::V_1_9_4 => 110,
            Self::V_1_10 | Self::V_1_10_1 | Self::V_1_10_2 => 210,
            Self::V_1_11 => 315,
            Self::V_1_11_1 | Self::V_1_11_2 => 316,
            Self::V_1_12 => 335,
            Self::V_1_12_1 => 338,
            Self::V_1_12_2 => 340,
            Self::V_1_13 => 393,
            Self::V_1_13_1 => 401,
            Self::V_1_13_2 => 404,
            Self::V_1_14 => 477,
            Self::V_1_14_1 => 480,
            Self::V_1_14_2 => 485,
            Self::V_1_14_3 => 490,
            Self::V_1_14_4 => 498,
            Self::V_1_15 => 573,
            Self::V_1_15_1 => 575,
            Self::V_1_15_2 => 578,
            Self::V_1_16 => 735,
            Self::V_1_16_1 => 736,
            Self::V_1_16_2 => 751,
            Self::V_1_16_3 => 753,
            Self::V_1_16_4 | Self::V_1_16_5 => 754,
            Self::V_1_17 => 755,
            Self::V_1_17_1 => 756,
            Self::V_1_18 | Self::V_1_18_1 => 757,
            Self::V_1_18_2 => 758,
            Self::V_1_19 => 759,
            Self::V_1_19_1 | Self::V_1_19_2 => 760,
            Self::V_1_19_3 => 761,
            Self::V_1_19_4 => 762,
            Self::V_1_20 | Self::V_1_20_1 => 763,
            Self::V_1_20_2 => 764,
            Self::V_1_20_3 | Self::V_1_20_4 => 765,
            Self::V_1_20_5 | Self::V_1_20_6 => 766,
            Self::V_1_21 | Self::V_1_21_1 => 767,
            Self::V_1_21_2 | Self::V_1_21_3 => 768,
            Self::V_1_21_4 => 769,
            Self::V_1_21_5 => 770,
            Self::V_1_21_6 => 771,
            Self::V_1_21_7 | Self::V_1_21_8 => 772,
            Self::V_1_21_9 | Self::V_1_21_10 => 773,
            Self::V_1_21_11 => 774,
            Self::V_26_1 | Self::V_26_1_1 | Self::V_26_1_2 => 775,
            Self::V_26_2 => 776,
            Self::V_26_3 => 777,
            Self::Unknown => -1,
        }
    }

    /// Resolves a version from a network protocol number.
    ///
    /// Returns [`JavaMinecraftVersion::Unknown`] if the protocol is not supported.
    #[must_use]
    pub const fn from_protocol(protocol: u32) -> Self {
        match protocol {
            4 => Self::V_1_7_2,
            5 => Self::V_1_7_6,
            22 => Self::V_1_0,
            23 => Self::V_1_1,
            29 => Self::V_1_2_5,
            39 => Self::V_1_3_2,
            47 => Self::V_1_8,
            51 => Self::V_1_4_7,
            61 => Self::V_1_5_2,
            78 => Self::V_1_6_4,
            107 => Self::V_1_9,
            108 => Self::V_1_9_1,
            109 => Self::V_1_9_2,
            110 => Self::V_1_9_3,
            210 => Self::V_1_10,
            315 => Self::V_1_11,
            316 => Self::V_1_11_1,
            335 => Self::V_1_12,
            338 => Self::V_1_12_1,
            340 => Self::V_1_12_2,
            393 => Self::V_1_13,
            401 => Self::V_1_13_1,
            404 => Self::V_1_13_2,
            477 => Self::V_1_14,
            480 => Self::V_1_14_1,
            485 => Self::V_1_14_2,
            490 => Self::V_1_14_3,
            498 => Self::V_1_14_4,
            573 => Self::V_1_15,
            575 => Self::V_1_15_1,
            578 => Self::V_1_15_2,
            735 => Self::V_1_16,
            736 => Self::V_1_16_1,
            751 => Self::V_1_16_2,
            753 => Self::V_1_16_3,
            754 => Self::V_1_16_4,
            755 => Self::V_1_17,
            756 => Self::V_1_17_1,
            757 => Self::V_1_18,
            758 => Self::V_1_18_2,
            759 => Self::V_1_19,
            760 => Self::V_1_19_1,
            761 => Self::V_1_19_3,
            762 => Self::V_1_19_4,
            763 => Self::V_1_20,
            764 => Self::V_1_20_2,
            765 => Self::V_1_20_3,
            766 => Self::V_1_20_5,
            767 => Self::V_1_21,
            768 => Self::V_1_21_2,
            769 => Self::V_1_21_4,
            770 => Self::V_1_21_5,
            771 => Self::V_1_21_6,
            772 => Self::V_1_21_7,
            773 => Self::V_1_21_9,
            774 => Self::V_1_21_11,
            775 => Self::V_26_1,
            776 => Self::V_26_2,
            777 => Self::V_26_3,
            _ => Self::Unknown,
        }
    }

    #[inline]
    #[must_use]
    pub const fn supports_configuration_state(&self) -> bool {
        self.protocol_version() >= Self::V_1_20_2.protocol_version()
    }

    #[inline]
    #[must_use]
    pub const fn is_modern(&self) -> bool {
        self.protocol_version() >= Self::V_1_13.protocol_version()
    }

    #[inline]
    #[must_use]
    pub const fn has_registries(&self) -> bool {
        self.protocol_version() >= Self::V_1_16.protocol_version()
    }
}

impl std::fmt::Display for JavaMinecraftVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V_1_0 => write!(f, "1.0"),
            Self::V_1_1 => write!(f, "1.1"),
            Self::V_1_2_5 => write!(f, "1.2.5"),
            Self::V_1_3_2 => write!(f, "1.3.2"),
            Self::V_1_4_7 => write!(f, "1.4.7"),
            Self::V_1_5_2 => write!(f, "1.5.2"),
            Self::V_1_6_4 => write!(f, "1.6.4"),
            Self::V_1_7_2 => write!(f, "1.7.2"),
            Self::V_1_7_3 => write!(f, "1.7.3"),
            Self::V_1_7_4 => write!(f, "1.7.4"),
            Self::V_1_7_5 => write!(f, "1.7.5"),
            Self::V_1_7_6 => write!(f, "1.7.6"),
            Self::V_1_7_7 => write!(f, "1.7.7"),
            Self::V_1_7_8 => write!(f, "1.7.8"),
            Self::V_1_7_9 => write!(f, "1.7.9"),
            Self::V_1_7_10 => write!(f, "1.7.10"),
            Self::V_1_8 => write!(f, "1.8"),
            Self::V_1_8_1 => write!(f, "1.8.1"),
            Self::V_1_8_2 => write!(f, "1.8.2"),
            Self::V_1_8_3 => write!(f, "1.8.3"),
            Self::V_1_8_4 => write!(f, "1.8.4"),
            Self::V_1_8_5 => write!(f, "1.8.5"),
            Self::V_1_8_6 => write!(f, "1.8.6"),
            Self::V_1_8_7 => write!(f, "1.8.7"),
            Self::V_1_8_8 => write!(f, "1.8.8"),
            Self::V_1_8_9 => write!(f, "1.8.9"),
            Self::V_1_9 => write!(f, "1.9"),
            Self::V_1_9_1 => write!(f, "1.9.1"),
            Self::V_1_9_2 => write!(f, "1.9.2"),
            Self::V_1_9_3 => write!(f, "1.9.3"),
            Self::V_1_9_4 => write!(f, "1.9.4"),
            Self::V_1_10 => write!(f, "1.10"),
            Self::V_1_10_1 => write!(f, "1.10.1"),
            Self::V_1_10_2 => write!(f, "1.10.2"),
            Self::V_1_11 => write!(f, "1.11"),
            Self::V_1_11_1 => write!(f, "1.11.1"),
            Self::V_1_11_2 => write!(f, "1.11.2"),
            Self::V_1_12 => write!(f, "1.12"),
            Self::V_1_12_1 => write!(f, "1.12.1"),
            Self::V_1_12_2 => write!(f, "1.12.2"),
            Self::V_1_13 => write!(f, "1.13"),
            Self::V_1_13_1 => write!(f, "1.13.1"),
            Self::V_1_13_2 => write!(f, "1.13.2"),
            Self::V_1_14 => write!(f, "1.14"),
            Self::V_1_14_1 => write!(f, "1.14.1"),
            Self::V_1_14_2 => write!(f, "1.14.2"),
            Self::V_1_14_3 => write!(f, "1.14.3"),
            Self::V_1_14_4 => write!(f, "1.14.4"),
            Self::V_1_15 => write!(f, "1.15"),
            Self::V_1_15_1 => write!(f, "1.15.1"),
            Self::V_1_15_2 => write!(f, "1.15.2"),
            Self::V_1_16 => write!(f, "1.16"),
            Self::V_1_16_1 => write!(f, "1.16.1"),
            Self::V_1_16_2 => write!(f, "1.16.2"),
            Self::V_1_16_3 => write!(f, "1.16.3"),
            Self::V_1_16_4 => write!(f, "1.16.4"),
            Self::V_1_16_5 => write!(f, "1.16.5"),
            Self::V_1_17 => write!(f, "1.17"),
            Self::V_1_17_1 => write!(f, "1.17.1"),
            Self::V_1_18 => write!(f, "1.18"),
            Self::V_1_18_1 => write!(f, "1.18.1"),
            Self::V_1_18_2 => write!(f, "1.18.2"),
            Self::V_1_19 => write!(f, "1.19"),
            Self::V_1_19_1 => write!(f, "1.19.1"),
            Self::V_1_19_2 => write!(f, "1.19.2"),
            Self::V_1_19_3 => write!(f, "1.19.3"),
            Self::V_1_19_4 => write!(f, "1.19.4"),
            Self::V_1_20 => write!(f, "1.20"),
            Self::V_1_20_1 => write!(f, "1.20.1"),
            Self::V_1_20_2 => write!(f, "1.20.2"),
            Self::V_1_20_3 => write!(f, "1.20.3"),
            Self::V_1_20_4 => write!(f, "1.20.4"),
            Self::V_1_20_5 => write!(f, "1.20.5"),
            Self::V_1_20_6 => write!(f, "1.20.6"),
            Self::V_1_21 => write!(f, "1.21"),
            Self::V_1_21_1 => write!(f, "1.21.1"),
            Self::V_1_21_2 => write!(f, "1.21.2"),
            Self::V_1_21_3 => write!(f, "1.21.3"),
            Self::V_1_21_4 => write!(f, "1.21.4"),
            Self::V_1_21_5 => write!(f, "1.21.5"),
            Self::V_1_21_6 => write!(f, "1.21.6"),
            Self::V_1_21_7 => write!(f, "1.21.7"),
            Self::V_1_21_8 => write!(f, "1.21.8"),
            Self::V_1_21_9 => write!(f, "1.21.9"),
            Self::V_1_21_10 => write!(f, "1.21.10"),
            Self::V_1_21_11 => write!(f, "1.21.11"),
            Self::V_26_1 => write!(f, "26.1"),
            Self::V_26_1_1 => write!(f, "26.1.1"),
            Self::V_26_1_2 => write!(f, "26.1.2"),
            Self::V_26_2 => write!(f, "26.2"),
            Self::V_26_3 => write!(f, "26.3"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Represents a specific version of the Minecraft Bedrock Edition protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[allow(non_camel_case_types)]
pub enum BedrockMinecraftVersion {
    /// 1.21: Tricky Trials.
    V_1_21,
    /// 1.26.51. Shares its protocol version with 1.26.50.
    V_1_26_51,
    /// Fallback for unrecognized protocol versions.
    Unknown,
}

impl BedrockMinecraftVersion {
    /// Returns the network protocol number for this version.
    ///
    /// Returns `-1` for [`BedrockMinecraftVersion::Unknown`].
    #[must_use]
    pub const fn protocol_version(&self) -> i32 {
        match self {
            Self::V_1_21 => 671,
            Self::V_1_26_51 => 2193,
            Self::Unknown => -1,
        }
    }

    /// Resolves a version from a network protocol number.
    ///
    /// Returns [`BedrockMinecraftVersion::Unknown`] if the protocol is not supported.
    #[must_use]
    pub const fn from_protocol(protocol: u32) -> Self {
        match protocol {
            671 => Self::V_1_21,
            2193 => Self::V_1_26_51,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for BedrockMinecraftVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V_1_21 => write!(f, "1.21"),
            Self::V_1_26_51 => write!(f, "1.26.51"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BedrockMinecraftVersion, JavaMinecraftVersion};

    #[test]
    fn resolves_named_java_releases() {
        let classic = JavaMinecraftVersion::from_protocol(22);
        assert_eq!(classic, JavaMinecraftVersion::V_1_0);
        assert_eq!(classic.to_string(), "1.0");
        assert_eq!(classic.protocol_version(), 22);

        let legacy = JavaMinecraftVersion::from_protocol(78);
        assert_eq!(legacy, JavaMinecraftVersion::V_1_6_4);
        assert_eq!(legacy.to_string(), "1.6.4");
        assert_eq!(legacy.protocol_version(), 78);

        let netty = JavaMinecraftVersion::from_protocol(4);
        assert_eq!(netty, JavaMinecraftVersion::V_1_7_2);
        assert_eq!(netty.to_string(), "1.7.2");
        assert_eq!(netty.protocol_version(), 4);

        let current = JavaMinecraftVersion::from_protocol(777);
        assert_eq!(current, JavaMinecraftVersion::V_26_3);
        assert_eq!(current.to_string(), "26.3");
        assert_eq!(current.protocol_version(), 777);

        assert!(JavaMinecraftVersion::V_1_0 < JavaMinecraftVersion::V_1_7_2);
        assert!(JavaMinecraftVersion::V_1_7_2 < JavaMinecraftVersion::V_26_3);
        assert!(JavaMinecraftVersion::V_26_3 < JavaMinecraftVersion::Unknown);
        assert_eq!(JavaMinecraftVersion::Unknown.protocol_version(), -1);

        assert_eq!(
            JavaMinecraftVersion::from_protocol(754),
            JavaMinecraftVersion::V_1_16_4
        );
        assert_eq!(JavaMinecraftVersion::V_1_16_5.protocol_version(), 754);
        assert_eq!(JavaMinecraftVersion::V_1_16_5.to_string(), "1.16.5");
        assert!(JavaMinecraftVersion::V_1_16_4 < JavaMinecraftVersion::V_1_16_5);
    }

    #[test]
    fn resolves_bedrock_26_51_protocol() {
        let version = BedrockMinecraftVersion::from_protocol(2193);

        assert_eq!(version, BedrockMinecraftVersion::V_1_26_51);
        assert_eq!(version.protocol_version(), 2193);
        assert_eq!(version.to_string(), "1.26.51");
    }
}
