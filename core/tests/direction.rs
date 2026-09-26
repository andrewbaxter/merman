use merman_core::{
    direction::{
        DirectionConvert,
        UnconvertAxis,
    },
    spec::SpecDirection::*,
};

#[test]
fn unconvert_places_boxes_per_direction() {
    // A 10x4 box at converse 100, transverse 20.
    let rd = DirectionConvert::new(Right, Down);
    assert_eq!(rd.direction_unconvert(100., 20., 10., 4.), (100., 20.));
    let ld = DirectionConvert::new(Left, Down);
    assert_eq!(ld.direction_unconvert(100., 20., 10., 4.), (-110., 20.));
    let ru = DirectionConvert::new(Right, Up);
    assert_eq!(ru.direction_unconvert(100., 20., 10., 4.), (100., -24.));

    // Vertical text: the box is 4 wide (transverse) and 10 tall (converse).
    let dr = DirectionConvert::new(Down, Right);
    assert_eq!(dr.direction_unconvert(100., 20., 4., 10.), (20., 100.));
    let ul = DirectionConvert::new(Up, Left);
    assert_eq!(ul.direction_unconvert(100., 20., 4., 10.), (-24., -110.));
    assert_eq!(dr.direction_unconvert_span(100., 20.), (20., 100.));
    assert_eq!(dr.direction_convert_span(1200., 900.), (900., 1200.));
    assert_eq!(ld.direction_unconvert_transverse(20., 4., 4.), UnconvertAxis {
        x: false,
        amount: 20.
    });
    assert_eq!(ul.direction_unconvert_transverse(20., 4., 4.), UnconvertAxis {
        x: true,
        amount: -24.
    });
}
