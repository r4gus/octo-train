use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use std::collections::HashMap;
use nodus::world2d::interaction2d::*;
use std::sync::atomic::{AtomicI32, Ordering};
use nodus::world2d::camera2d::MouseWorldPos;
use bevy_egui::{egui, EguiContext};

pub struct NodePlugin;

const NODE_GROUP: u32 = 1;
const CONNECTOR_GROUP: u32 = 2;

macro_rules! trans {
    ( $( $fun:expr ),* ) => {
        vec![ $( Box::new($fun) ),* ]
    };
    ( $( $fun:expr ),+ ,) => {
        trans![ $( $fun ),* ]
    };
}

impl Plugin for NodePlugin {
    fn build(&self, app: &mut AppBuilder) {
        // add things to the app here
        //.add_system(hello_world.system())
        //.add_system(greet_node.system())
        app.add_startup_system(setup.system())
            .add_event::<ConnectEvent>()
            .add_event::<ChangeInput>()
            .add_event::<DisconnectEvent>()
            .add_system(transition_system.system().label("transition"))
            .add_system(propagation_system.system().after("transition"))
            .add_system(highlight_connector_system.system())
            .add_system(drag_gate_system.system())
            .add_system(drag_connector_system.system().label("drag_conn_system"))
            .add_system(connect_nodes.system().after("drag_conn_system"))
            .add_system(draw_line_system.system())
            .add_system(ui_node_info_system.system())
            .add_system(change_input_system.system())
            .add_system(disconnect_event.system());
        
        info!("NodePlugin loaded");
    }
}

/// The name of an entity.
pub struct Name(String);

/// The input and output states of a logic gate.
///
/// # States
/// `None` - The state is unknown, for example because the gate
/// doesn't get a value for each input.
/// `High` - The sate is high (`1`).
/// `Low` - The state is low (`0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    None,
    High,
    Low,
}

/// System stages to group systems related to the
/// node module.
#[derive(Debug, Hash, PartialEq, Eq, Clone, StageLabel)]
enum NodeStages {
    Update
}

/// Labels for the different systems of this module.
/// The labels are used to force an explicit ordering
/// between the systems when neccessary.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
enum NodeLabels {
    Transition
}

/// Type that maps form an (gate) entity to it's 
/// connected inputs.
type TargetMap = HashMap<Entity, Vec<usize>>;

#[derive(Debug, Copy, Clone)]
pub struct NodeRange {
    min: u32,
    max: u32
}

/// Flag for logic gates.
pub struct Gate {
    pub inputs: u32,
    pub outputs: u32,
    pub in_range: NodeRange,
    pub out_range: NodeRange,
}

const GATE_SIZE: f32 = 100.;

struct GateSize {
    width: f32,
    height: f32,
    in_step: f32,
    out_step: f32,
    offset: f32,
}

static Z_INDEX: AtomicI32 = AtomicI32::new(1);

impl Gate {

    fn get_distances(cin: f32, cout: f32) -> GateSize {
        let factor = if cin >= cout { cin } else { cout };
        let width = GATE_SIZE;
        let height = GATE_SIZE + if factor > 2. {
            (factor - 1.) * GATE_SIZE / 2.
        } else { 0. };
        let in_step = -(height / (cin + 1.));
        let out_step = -(height / (cout + 1.));
        let offset = height / 2.;

        GateSize {
            width,
            height,
            in_step,
            out_step,
            offset
        }
    }

    pub fn new(
        commands: &mut Commands, 
        name: String,
        x: f32, y: f32, 
        in_range: NodeRange, 
        out_range: NodeRange,
        functions: Vec<Box<dyn Fn(&[State]) -> State + Send + Sync>>
    ) { 
        let dists = Gate::get_distances(in_range.min as f32, out_range.min as f32);

        let zidx = Z_INDEX.fetch_add(1, Ordering::Relaxed) as f32;
        let shape = shapes::Rectangle {
            width: dists.width,
            height: dists.height,
            ..shapes::Rectangle::default()
        };
        let gate = GeometryBuilder::build_as(
            &shape,
            ShapeColors::outlined(Color::TEAL, Color::BLACK),
            DrawMode::Outlined {
                fill_options: FillOptions::default(),
                outline_options: StrokeOptions::default().with_line_width(10.0),
            },
            Transform::from_xyz(x, y, zidx),
        );
        let parent = commands
            .spawn_bundle(gate)
            .insert(Gate { 
                inputs: in_range.min,
                outputs: out_range.min,
                in_range,
                out_range
            })
            .insert(Name(name))
            .insert(Inputs(vec![State::None; in_range.min as usize]))
            .insert(Outputs(vec![State::None; out_range.min as usize]))
            .insert(Transitions(functions))
            .insert(Targets(vec![HashMap::new(); out_range.min as usize]))
            .insert(Interactable::new(Vec2::new(0., 0.), Vec2::new(dists.width, dists.height), NODE_GROUP))
            .insert(Selectable)
            .insert(Draggable { update: true })
            .id();
        
        let mut entvec: Vec<Entity> = Vec::new();
        for i in 1..=in_range.min {
            entvec.push(Connector::new(commands, 
                                       Vec3::new(-75., dists.offset + i as f32 * dists.in_step, zidx), 
                                       12., 
                                       ConnectorType::In,
                                       (i - 1) as usize));
        }

        commands.entity(parent).push_children(&entvec);
        entvec.clear();

        for i in 1..=out_range.min {
            entvec.push(Connector::new(commands, 
                                       Vec3::new(75., dists.offset + i as f32 * dists.out_step, zidx), 
                                       12., 
                                       ConnectorType::Out,
                                       (i - 1) as usize));
        }
        commands.entity(parent).push_children(&entvec);
    }

    pub fn constant(
        commands: &mut Commands, 
        name: String,
        x: f32, y: f32,
        state: State,
    ) {
        let dists = Gate::get_distances(1., 1.);

        let zidx = Z_INDEX.fetch_add(1, Ordering::Relaxed) as f32;
        let shape = shapes::Rectangle {
            width: dists.width,
            height: dists.height,
            ..shapes::Rectangle::default()
        };
        let gate = GeometryBuilder::build_as(
            &shape,
            ShapeColors::outlined(Color::TEAL, Color::BLACK),
            DrawMode::Outlined {
                fill_options: FillOptions::default(),
                outline_options: StrokeOptions::default().with_line_width(10.0),
            },
            Transform::from_xyz(x, y, zidx),
        );
        let parent = commands
            .spawn_bundle(gate)
            .insert(Gate { 
                inputs: 1,
                outputs: 1,
                in_range: NodeRange { min: 1, max: 1 },
                out_range: NodeRange { min: 1, max: 1 },
            })
            .insert(Name(name))
            .insert(Inputs(vec![state]))
            .insert(Outputs(vec![State::None]))
            .insert(Transitions(trans![|inputs| inputs[0]]))
            .insert(Targets(vec![HashMap::new()]))
            .insert(Interactable::new(Vec2::new(0., 0.), Vec2::new(dists.width, dists.height), NODE_GROUP))
            .insert(Selectable)
            .insert(Draggable { update: true })
            .id();
        
        let mut entvec: Vec<Entity> = Vec::new();
        entvec.push(Connector::new(commands, 
                                   Vec3::new(75., dists.offset + dists.out_step, zidx), 
                                   12., 
                                   ConnectorType::Out,
                                   0));
        commands.entity(parent).push_children(&entvec);
    }
}

/// Input values of a logical node, e.g. a gate.
pub struct Inputs(Vec<State>);

/// Output values of a logical node, e.g. a gate.
pub struct Outputs(Vec<State>);

/// A set of transition functions `f: Inputs -> State`.
///
/// For a logic node, e.g. a gate, there should be a transition function
/// for each output.
pub struct Transitions(Vec<Box<dyn Fn(&[State]) -> State + Send + Sync>>);

/// A vector that maps from outputs to connected nodes.
///
/// For a logic node, e.g. a gate, there should be a vector entry for
/// each output.
pub struct Targets(Vec<TargetMap>);

/// System for calculating the state of each output using the corresponding
/// transition functions.
fn transition_system(mut query: Query<(&Inputs, &Transitions, &mut Outputs)>) {
    for (inputs, transitions, mut outputs) in query.iter_mut() {
        for i in 0..transitions.0.len() {
            outputs.0[i] = transitions.0[i](&inputs.0);
        }
    }
}

/// System for writing the calculated output states to the inputs of each connected node.
fn propagation_system(from_query: Query<(&Outputs, &Targets)>, mut to_query: Query<&mut Inputs>) {
    for (outputs, targets) in from_query.iter() {
        for i in 0..outputs.0.len() {
            for (entity, idxvec) in &targets.0[i] {
                if let Ok(mut inputs) = to_query.get_component_mut::<Inputs>(*entity) {
                    for &j in idxvec {
                        if j < inputs.0.len() {
                            inputs.0[j] = outputs.0[i];
                        }
                    }
                } else {
                    error!("Could not query inputs of given entity ");
                }
            }
        }
    }
}


fn setup(mut commands: Commands) {
    Gate::new(&mut commands, 
              "NOT Gate".to_string(), 
              0., 0., 
              NodeRange { min: 1, max: 1 },
              NodeRange { min: 1, max: 1 },
              trans![|inputs| {
                match inputs[0] {
                    State::None => State::None,
                    State::Low => State::High,
                    State::High => State::Low,
                }
              },]
              );

    Gate::new(&mut commands, 
              "AND Gate".to_string(), 
              250., 0., 
              NodeRange { min: 2, max: 16 },
              NodeRange { min: 1, max: 1 },
              trans![|inputs| {
                  let mut ret = State::High;
                  for i in inputs {
                    match i {
                        State::None => { ret = State::None; },
                        State::Low => { ret = State::Low; break; },
                        State::High => { },
                    }
                  }
                  ret
              },]
            );

    Gate::new(&mut commands, 
              "OR Gate".to_string(), 
              500., 0., 
              NodeRange { min: 2, max: 16 },
              NodeRange { min: 1, max: 1 },
              trans![|inputs| {
                  let mut ret = State::Low;
                  for i in inputs {
                    match i {
                        State::None => { ret = State::None; },
                        State::Low => {  },
                        State::High => { ret = State::High; break; },
                    }
                  }
                  ret
              },]
            );

    Gate::constant(&mut commands,
                   "HIGH Const".to_string(),
                   -200., 200.,
                   State::High);

    Gate::constant(&mut commands,
                   "LOW Const".to_string(),
                   -200., -200.,
                   State::Low);
}


fn drag_gate_system(
    mut commands: Commands,
    mb: Res<Input<MouseButton>>,
    q_dragged: Query<Entity, (With<Drag>, With<Gate>)>
) {
    if mb.just_released(MouseButton::Left) {
        for dragged_gate in q_dragged.iter() {
            commands.entity(dragged_gate).remove::<Drag>();
        }
    }
}

// ############################# Connector ##############################################

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConnectorType {
    In,
    Out 
}

/// A connector acts as the interface between two nodes, e.g. a logic gate.
pub struct Connector {
    /// The type of the connector.
    ctype: ConnectorType,
    /// Its index in context of a logical node.
    index: usize,
}

/// Connection lines connected to this connector.
pub struct Connections(Vec<Entity>);

pub struct Free;

impl Connector {
    /// Create a new connector for a logic node.
    pub fn new(commands: &mut Commands, position: Vec3, radius: f32, ctype: ConnectorType, index: usize) -> Entity {
        let circle = shapes::Circle {
            radius: radius,
            center: Vec2::new(0., 0.),
        };

        let connector = GeometryBuilder::build_as(
            &circle,
            ShapeColors::outlined(Color::TEAL, Color::BLACK),
            DrawMode::Outlined {
                fill_options: FillOptions::default(),
                outline_options: StrokeOptions::default().with_line_width(5.0),
            },
            Transform::from_xyz(position.x, position.y, position.z),
        );

        commands
            .spawn_bundle(connector)
            .insert(Connector { 
                ctype,
                index
            })
            .insert(Connections(Vec::new()))
            .insert(Free)
            .insert(Interactable::new(Vec2::new(0., 0.), Vec2::new(radius * 2., radius * 2.), 
                                      CONNECTOR_GROUP))
            .insert(Selectable)
            .insert(Draggable { update: false })
            .id()
    }
}

/// Highlight a connector by increasing its radius when the mouse
/// hovers over it.
fn highlight_connector_system(
    commands: Commands,
    // We need all connectors the mouse hovers over.
    mut q_hover: Query<&mut Transform, (With<Hover>, With<Connector>)>,
    mut q2_hover: Query<&mut Transform, (Without<Hover>, With<Connector>)>,
) { 
    for (mut transform) in q_hover.iter_mut() {
        transform.scale = Vec3::new(1.2, 1.2, transform.scale.z);
    }

    for (mut transform) in q2_hover.iter_mut() {
        transform.scale = Vec3::new(1.0, 1.0, transform.scale.z);
    }
}

/// A line shown when the user clicks and drags from a connector.
/// It's expected that there is atmost one ConnectionLineIndicator
/// present.
pub struct ConnectionLineIndicator;

fn drag_connector_system(
    mut commands: Commands,
    mb: Res<Input<MouseButton>>,
    mw: Res<MouseWorldPos>,
    // ID and transform of the connector we drag from.
    q_dragged: Query<(Entity, &GlobalTransform, &Connector), (With<Drag>, With<Free>)>,
    // The visual connection line indicator to update.
    q_conn_line: Query<Entity, With<ConnectionLineIndicator>>,
    // Posible free connector the mouse currently hovers over.
    q_drop: Query<(Entity, &Connector), (With<Hover>, With<Free>)>,
    mut ev_connect: EventWriter<ConnectEvent>,
) {
    use bevy_prototype_lyon::entity::ShapeBundle;

    if let Ok((entity, transform, connector)) = q_dragged.single() {
        // If the LMB is released we check if we can connect two connectors.
        if mb.just_released(MouseButton::Left) {
            commands.entity(entity).remove::<Drag>();

            // We dont need the visual connection line any more.
            // There will be another system responsible for
            // drawing the connections between nodes.
            if let Ok(conn_line) = q_conn_line.single() {
                commands.entity(conn_line).despawn();
            }

            // Try to connect input and output.
            if let Ok((drop_target, drop_connector)) = q_drop.single() {
                // One can only connect an input to an output.
                if connector.ctype != drop_connector.ctype {
                    // Send connection event.
                    match connector.ctype {
                        ConnectorType::In => {
                            ev_connect.send( 
                                ConnectEvent {
                                    output: drop_target,
                                    output_index: drop_connector.index,
                                    input: entity,
                                    input_index: connector.index
                                }
                            );
                        },
                        ConnectorType::Out => {
                            ev_connect.send(
                                ConnectEvent {
                                    output: entity,
                                    output_index: connector.index,
                                    input: drop_target,
                                    input_index: drop_connector.index,
                                }
                            );
                        }
                    }
                }
            }
        } else {
        // While LMB is being pressed, draw the line from the node clicked on
        // to the mouse cursor.
            let conn_entity = if let Ok(conn_line) = q_conn_line.single() {
                commands.entity(conn_line).remove_bundle::<ShapeBundle>();
                conn_line
            } else {
                commands.spawn().insert(ConnectionLineIndicator).id()  
            };

            let shape = shapes::Line(Vec2::new(transform.translation.x, transform.translation.y), 
                                     Vec2::new(mw.x, mw.y));

            let line = GeometryBuilder::build_as(
                &shape,
                ShapeColors::outlined(Color::TEAL, Color::BLACK),
                DrawMode::Outlined {
                    fill_options: FillOptions::default(),
                    outline_options: StrokeOptions::default().with_line_width(10.0),
                },
                Transform::from_xyz(0., 0., 1.),
            );

            commands.entity(conn_entity).insert_bundle(line);
        }
        
    }
}

struct ConnectEvent {
    output: Entity,
    output_index: usize,
    input: Entity,
    input_index: usize
}

/// Handle incomming connection events.
fn connect_nodes(
    mut commands: Commands,
    mut ev_connect: EventReader<ConnectEvent>,
    mut q_conns: Query<(&Parent, &mut Connections), ()>,
    mut q_parent: Query<&mut Targets>,
    q_transform: Query<&GlobalTransform, ()>,
) {
    for ev in ev_connect.iter() {
        eprintln!("connect");
        let line = ConnectionLine::new(
            &mut commands,
            ConnInfo {
                entity: ev.output,
                index: ev.output_index            
            },
            ConnInfo {
                entity: ev.input,
                index: ev.input_index
            },
            (q_transform.get(ev.output).unwrap().translation, q_transform.get(ev.input).unwrap().translation),
        );

        let input_parent = if let Ok((parent, mut connections)) = q_conns.get_mut(ev.input) {
            connections.0.push(line);
            parent.0
        } else { continue };
        commands.entity(ev.input).remove::<Free>();

        if let Ok((parent, mut connections)) = q_conns.get_mut(ev.output) {
            connections.0.push(line);

            if let Ok(mut targets) = q_parent.get_mut(parent.0) {
                targets.0[ev.output_index]
                    .entry(input_parent)
                    .or_insert(Vec::new())
                    .push(ev.input_index);
            }
        }
    }
}

struct DisconnectEvent {
    connection: Entity,
    in_parent: Option<Entity>,
}

fn disconnect_event(
    mut commands: Commands,
    mut ev_disconnect: EventReader<DisconnectEvent>,
    mut q_line: Query<(&Children, &ConnectionLine)>,
    mut q_conn: Query<(&Parent, Entity, &mut Connections)>,
    mut q_parent: Query<&mut Targets>,
) {
    for ev in ev_disconnect.iter() {
        eprintln!("disconnect");
        if let Ok((children, line)) = q_line.get(ev.connection) {
            let mut in_parent: Option<Entity> = None;

            // Unlink input connector (right hand side)
            if let Ok((parent_in, entity_in, mut connections_in)) = q_conn.get_mut(line.input.entity) {
                in_parent = Some(parent_in.0);

                // Clear the input line from the vector and
                // mark the connector as free.
                connections_in.0.clear(); 
                commands.entity(entity_in).insert(Free);
            } else {
                in_parent = ev.in_parent;
            }

            // Unlink output connector (left hand side)
            if let Ok((parent_out, entity_out, mut connections_out)) = q_conn.get_mut(line.output.entity) {
                let parent = in_parent.expect("There should always bee a parent set");
                
                // Find and remove the given connection line.
                if let Some(idx) = connections_out.0.iter().position(|x| *x == ev.connection) {
                    connections_out.0.remove(idx); 
                }

                // Unlink propagation target.
                // Find the index of the input connector within the
                // target map of the gate the output connector belongs
                // to and remove the associated entry.
                if let Ok(mut targets) = q_parent.get_mut(parent_out.0) {
                    if let Some(index) = targets.0[line.output.index]
                                    .get_mut(&parent).expect("Should have associated entry")
                                    .iter().position(|x| *x == line.input.index)
                    {
                        eprintln!("romove");
                        targets.0[line.output.index]
                            .get_mut(&parent).expect("Should have associated entry")
                            .remove(index);
                    }
                }
            }

            for &child in children.iter() {
                commands.entity(child).despawn_recursive();
            }
            
            // Finally remove the connection line itself.
            commands.entity(ev.connection).despawn();
        }
    }
}

// ############################# Connection Line ########################################

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ConnInfo {
    entity: Entity,
    index: usize,    
}

pub struct ConnectionLine {
    output: ConnInfo,
    via: Vec<Vec2>,
    input: ConnInfo,
}

impl ConnectionLine {
    pub fn new(commands: &mut Commands, output: ConnInfo, input: ConnInfo, positions: (Vec3, Vec3)) -> Entity {
        commands
            .spawn()
            .insert(ConnectionLine {
                output,
                via: ConnectionLine::calculate_nodes(positions.0.x, positions.0.y, positions.1.x, positions.1.y),
                input,
            }).id()
    }

    /// Calculate the nodes of a path between two points.
    pub fn calculate_nodes(x1: f32, y1: f32, x2: f32, y2: f32) -> Vec<Vec2> {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let dx2 = dx / 2.;
        let dy2 = dy / 2.;
        let point1 = Vec2::new(x1, y1);
        let point2 = if dx >= 0. {
            Vec2::new(x1 + dx2, y1)
        } else {
            Vec2::new(x1, y1 + dy2)
        };
        let point3 = if dx >= 0. {
            Vec2::new(x1 + dx2, y1 + dy)
        } else {
            Vec2::new(x1 + dx, y1 + dy2)
        };
        let point4 = Vec2::new(x1 + dx, y1 + dy);

        vec![point1, point2, point3, point4]
    }
}

fn draw_line_system(
    mut commands: Commands,
    q_line: Query<(Entity, &ConnectionLine), ()>,
    q_transform: Query<(&Parent, &Connector, &GlobalTransform), ()>,
    q_outputs: Query<&Outputs, ()>,
    q_children: Query<&Children>,
) {
    use bevy_prototype_lyon::entity::ShapeBundle;

    for (entity, conn_line) in q_line.iter() {
        if let Ok((t_parent, t_conn, t_from)) = q_transform.get(conn_line.output.entity) {
            // Set connection line color based on the value of the output.
            let color = if let Ok(outputs) = q_outputs.get(t_parent.0) {
                match outputs.0[t_conn.index] {
                    State::None => Color::RED,
                    State::High => Color::BLUE,
                    State::Low => Color::BLACK,
                }
            } else {
                Color::BLACK
            };

            if let Ok((_, _, t_to)) = q_transform.get(conn_line.input.entity) {
                // Remove old line
                if let Ok(children) = q_children.get(entity) {
                    for &child in children.iter() {
                        commands.entity(child).despawn();
                    }
                }
                let via = ConnectionLine::calculate_nodes(t_from.translation.x, t_from.translation.y, t_to.translation.x, t_to.translation.y);
                let mut segments: Vec<Entity> = Vec::new();
                for i in 0..(via.len() - 1) {
                    // Insert new line
                    let shape = shapes::Line(Vec2::new(via[i].x, via[i].y), 
                                             Vec2::new(via[i+1].x, via[i+1].y));

                    let line = GeometryBuilder::build_as(
                        &shape,
                        ShapeColors::outlined(Color::TEAL, color),
                        DrawMode::Outlined {
                            fill_options: FillOptions::default(),
                            outline_options: StrokeOptions::default().with_line_width(10.0),
                        },
                        Transform::from_xyz(0., 0., 1.),
                    );
                    
                    segments.push(commands.spawn_bundle(line).id());

                    // This hides the edges between two lines.
                    if i > 0 {
                        let circ_shape = shapes::Circle { radius: 4.0, center: Vec2::new(via[i].x, via[i].y) };

                        let circle = GeometryBuilder::build_as(
                            &circ_shape,
                            ShapeColors::outlined(color, color),
                            DrawMode::Outlined {
                                fill_options: FillOptions::default(),
                                outline_options: StrokeOptions::default(),
                            },
                            Transform::from_xyz(0., 0., 1.),
                        );
                        segments.push(commands.spawn_bundle(circle).id());
                    }
                }

                commands.entity(entity).push_children(&segments);
            }
        }
   }
}

// ############################# User Interface #########################################

struct ChangeInput {
    gate: Entity,
    to: u32,
}

fn ui_node_info_system(
    egui_context: ResMut<EguiContext>,
    q_gate: Query<(Entity, &Name, &Gate), With<Selected>>,
    mut ev_change: EventWriter<ChangeInput>,
) {
    for (entity, name, gate) in q_gate.iter() {
        egui::Window::new(&name.0).show(egui_context.ctx(), |ui| {
            if gate.in_range.min != gate.in_range.max {
                ui.horizontal(|ui| {
                    ui.label("Input Count: ");
                    if ui.button("➖").clicked() {
                        if gate.inputs > gate.in_range.min {
                            ev_change.send(
                                ChangeInput {
                                    gate: entity,
                                    to: gate.inputs - 1,
                                }
                            );
                        }
                    }
                    ui.label(format!("{}", gate.inputs));
                    if ui.button("➕").clicked() {
                        if gate.inputs < gate.in_range.max {
                            ev_change.send(
                                ChangeInput {
                                    gate: entity,
                                    to: gate.inputs + 1,
                                }
                            );
                        }
                    }
                });
            }
        });
    }
}

fn change_input_system(
    mut commands: Commands,
    mut ev_connect: EventReader<ChangeInput>,
    mut ev_disconnect: EventWriter<DisconnectEvent>,
    mut q_gate: Query<(Entity, &mut Gate, &mut Inputs, &mut Interactable, &GlobalTransform)>,
    mut q_connectors: Query<&Children>,
    mut q_connector: Query<(&mut Connector, &mut Transform, &Connections)>,
) {
    use bevy_prototype_lyon::entity::ShapeBundle;

    for ev in ev_connect.iter() {
        if let Ok((gent, mut gate, mut inputs, mut interact, transform)) = q_gate.get_mut(ev.gate) {
            // Update input count
            gate.inputs = ev.to;

            let translation = transform.translation;
            let dists = Gate::get_distances(gate.inputs as f32, gate.outputs as f32);

            // Update bounding box
            interact.update_size(0., 0., dists.width, dists.height);

            // Update input vector
            inputs.0.resize(gate.inputs as usize, State::None);

            let shape = shapes::Rectangle {
                width: dists.width,
                height: dists.height,
                ..shapes::Rectangle::default()
            };
            let gate = GeometryBuilder::build_as(
                &shape,
                ShapeColors::outlined(Color::TEAL, Color::BLACK),
                DrawMode::Outlined {
                    fill_options: FillOptions::default(),
                    outline_options: StrokeOptions::default().with_line_width(10.0),
                },
                Transform::from_xyz(translation.x, translation.y, translation.z),
            );
            
            // Update body
            commands.entity(ev.gate).remove_bundle::<ShapeBundle>();
            commands.entity(ev.gate).insert_bundle(gate);

            // Update connectors attached to this gate
            let mut max = 0;
            if let Ok(connectors) = q_connectors.get(ev.gate) {
                for connector in connectors.iter() {
                    if let Ok((conn, mut trans, conns)) = q_connector.get_mut(*connector) {
                        if conn.ctype == ConnectorType::In {
                            if conn.index < ev.to as usize {
                                trans.translation = Vec3::new(-75., 
                                    dists.offset + (conn.index + 1) as f32 * dists.in_step, 
                                    translation.z); 
                                if max < conn.index { max = conn.index; }
                            } else {
                                // Remove connector if neccessary. This includes logical
                                // links between gates and connection line entities.
                                for &c in &conns.0 {
                                    ev_disconnect.send(
                                        DisconnectEvent {
                                            connection: c,
                                            in_parent: Some(gent),
                                        }
                                    );
                                }

                                // Finally remove entity.
                                commands.entity(*connector).despawn();
                            }
                        }
                    }
                }
            }
       
            // If the expected amount of connectors exceeds the factual
            // amount, add new connectors to the gate.
            let mut entvec: Vec<Entity> = Vec::new();
            for i in (max + 1)..=ev.to as usize {
                entvec.push(Connector::new(&mut commands, 
                           Vec3::new(-75., dists.offset + i as f32 * dists.in_step, translation.z), 
                           12., 
                           ConnectorType::In,
                           (i - 1) as usize));
            }
            commands.entity(ev.gate).push_children(&entvec);
        }
        
    }
}