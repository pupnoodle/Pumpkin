use pumpkin_core::entity::projectile::arrow::ArrowEntity;
use pumpkin_core::entity::shield_blocks_facing;
use pumpkin_core::world::Explosion;
use pumpkin_util::math::vector3::Vector3;

#[test]
fn flying_arrow_hit_damage_is_positive() {
    let charged = ArrowEntity::arrow_hit_damage(3.0, 2.0);
    let slower = ArrowEntity::arrow_hit_damage(1.0, 2.0);
    assert!(charged > 0);
    assert!(slower > 0);
    assert!(charged > slower);
}

#[test]
fn explosion_damage_falls_off_with_distance() {
    let center = Explosion::explosion_damage_at(4.0, 0.0, 1.0);
    let mid = Explosion::explosion_damage_at(4.0, 0.5, 1.0);
    let edge = Explosion::explosion_damage_at(4.0, 1.0, 1.0);
    assert!(center > mid);
    assert!(mid > edge);
    assert!(center > 1.0);
}

#[test]
fn shield_blocks_attacker_in_front_and_not_behind() {
    let defender = Vector3::new(0.0, 64.0, 0.0);
    let in_front = Vector3::new(0.0, 64.0, 3.0);
    let behind = Vector3::new(0.0, 64.0, -3.0);
    assert!(shield_blocks_facing(defender, 0.0, in_front));
    assert!(!shield_blocks_facing(defender, 0.0, behind));
}
