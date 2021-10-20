#[derive(Debug, Clone, Eq, PartialEq, Hash, Reflect)]
enum CurveType {
    Linear
}



#[derive(Debug, Clone, Eq, PartialEq, Hash, Reflect, Component)]
pub struct CurveComponent {
    start: f32,
    stop: f32,
    amount_completed: f32,
    duration: f32,
    pub value: f32
    curve_type: CurveType,
    finished: bool
}


pub fn curve_update_system(
    mut curves: Query<(&mut CurveComponent)>
) { 
    for (mut curve) in curves.iter_mut() {
        match surve.curve_type {
            CurveType::Linear => {
                curve.value = (1.0 - curve.amount_completed) * curve.start + curve.amount_completed * curve.stop;
            }
        }
    }
}