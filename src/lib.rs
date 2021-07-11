use bevy::{
    app::{Events, ManualEventReader},
    ecs::schedule::SystemSet,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    render::{
        camera::Camera,
        camera::CameraProjection,
        camera::{ActiveCameras, PerspectiveProjection},
        render_graph::base::camera::CAMERA_3D,
    },
    window::Windows,
};

pub mod cam;
use cam::{MovementSettings, player_move};

pub mod player;
use player::{Player, PlayerSettings, move_player};

use strum::IntoEnumIterator;
use strum_macros::EnumIter;

pub struct PlayerMove;

#[macro_export]
macro_rules! next_enum {
    ($l:ident, $k:expr) => {
        $l::iter()
            .enumerate()
            .nth(
                $l::iter()
                    .enumerate()
                    .find(|a| a.1 == *$k.current())
                    .map(|(i, _)| {
                        if i + 1 > $l::iter().count() - 1 {
                            0usize
                        } else {
                            i + 1
                        }
                    })
                    .unwrap(),
            )
            .unwrap()
            .1
    };
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
enum PluginState {
    Enabled,
    //Disabled,
}

#[derive(Clone, Eq, PartialEq, Debug, Hash, EnumIter)]
enum ScrollType {
    MovementSpeed,
    Zoom,
    Sensitivity,
    Lerp,
    CamFwd,
}
#[derive(Clone, Eq, PartialEq, Debug, Hash, EnumIter)]
pub enum CameraMode {
    //Look at player and other targets if set
    LookAt,
    //Follow the player only
    FollowStatic,
    //Camera is moved above and pointed down, rotation bound to one axis
    TopDown,
    //Same as TopDown but follows the players direction
    TopDownDirection,
    //Follows behind the player a certain distance
    FollowBehind,
    //Camera at same position as player, enables to use the mouse to look (WIP)
    Fps,
    //Use the mouse to look and move the camera freely
    Free,
}

/// Used in queries when you want flycams and not other cameras

//Free moving camera
pub struct StaticCam;
//Attached to target
pub struct AttachedCam;

pub struct Config {
    pub current_camera_mode: usize,
    pub allowed_camera_modes: &'static [CameraMode],
    pub target: Option<Entity>,
    pub external_target: Option<Entity>,
    pub camera_settings: CameraSettings,
    pub controller_settings: Option<Controller>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            current_camera_mode: 0,
            allowed_camera_modes: &[
                CameraMode::LookAt,
                CameraMode::FollowStatic,
                CameraMode::TopDown,
                CameraMode::FollowBehind,
                CameraMode::Fps,
                CameraMode::Free,
            ],
            target: None,
            external_target: None,
            camera_settings: CameraSettings {
                mouse_sensitivity: 0.00012,
                speed: 12.,
                pos: Transform::from_xyz(-2.0, 5.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                map: 0.,
                camera: None,
                camera_should_focus: Vec3::default(),
                camera_is_focus: Vec3::default(),
                attached_camera: None,
            },
            controller_settings: Some(Controller {
                speed: 1.,
                rot_speed: 0.1,
                map: 0.,
            }),
        }
    }
}

pub struct CameraSettings {
    pub mouse_sensitivity: f32,
    pub speed: f32,
    pub pos: Transform,
    pub map: f32,
    pub camera: Option<Entity>,
    camera_should_focus: Vec3,
    camera_is_focus: Vec3,
    pub attached_camera: Option<Entity>,
}

pub struct Controller {
    pub speed: f32,
    pub rot_speed: f32,
    pub map: f32,
}

fn setup_camera(
    mut commands: Commands, 
    mut config: ResMut<Config>, 
){
    if config.camera_settings.camera.is_none(){
        config.camera_settings.camera = Some(
            commands
            .spawn_bundle(PerspectiveCameraBundle {
                camera: Camera {
                    name: Some("Camera3d".to_string()),
                    ..Default::default()
                },
                transform: config.camera_settings.pos,
                ..Default::default()
            })
            .insert(StaticCam).id()
        );
    } else {
        let mut e = commands.entity(config.camera_settings.camera.unwrap());
        config.camera_settings.camera = Some (e.insert(StaticCam).id());
    } 
}

fn setup_controller(
    mut commands: Commands, 
    mut config: ResMut<Config>, 
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
){
    //Create or Update Target
    let trans = Vec3::new(0.,0.5,0.);

    let player = if config.target.is_none() {
        commands.spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
            material: materials.add(Color::rgb(0.7, 0.3, 0.3).into()),
            transform: Transform::from_translation(trans),
            ..Default::default()
        }).id()
    } else {
        config.target.unwrap()
    };

    let a = commands.entity(player).insert(PlayerMove)
    .with_children(|parent|{
        parent
            .spawn_bundle(PerspectiveCameraBundle {
                camera: Camera {
                    name: Some("Target".to_string()),
                    ..Default::default()
                },
                transform: Transform::from_translation(trans)
                    .looking_at(Vec3::ZERO, Vec3::Y),
                ..Default::default()
            })
            .insert(AttachedCam);
    }).id();

    config.target = Some(a);
}

const RESET_FOCUS: [f32; 3] = [0., 0., 0.];

#[allow(unused_must_use)]
fn cycle_cam_state(
    //mut cam_state: ResMut<State<CameraMode>>,
    settings: Res<MovementSettings>,
    keyboard_input: Res<Input<KeyCode>>,
    mut config: ResMut<Config>,
) {
    if keyboard_input
        .get_just_pressed()
        .any(|m| settings.map.next_cam.iter().any(|nc| m == nc))
    {
        let current = &config.current_camera_mode;
        let available = config.allowed_camera_modes;
        let next = if available.len() - 1 > *current  {
            current + 1
        } else {
            0
        };

        config.current_camera_mode = next;        
        println!("Camera: {:?}", config.allowed_camera_modes.get(config.current_camera_mode).unwrap());
    }
}

// change the focus of the camera
#[allow(clippy::type_complexity)]
fn move_camera(
    time: Res<Time>,
    mut config: ResMut<Config>,
    mut settings: ResMut<MovementSettings>,
    mut transforms: QuerySet<(Query<(&mut Transform, &Camera)>, Query<&Transform>)>,
) {
    let mut delta_trans = Transform::identity();
    settings.disable_look = true;
    settings.disable_move = false;
    settings.locked_to_player = false;

    match config.allowed_camera_modes.get(config.current_camera_mode).unwrap() {
        CameraMode::Free => {
            settings.disable_look = false;
            return;
        }
        CameraMode::LookAt => {
            // if there is both a player and a bonus, target the mid-point of them
            if let (Some(player_entity), Some(bonus_entity)) = (config.target, config.external_target) {
                if let (Ok(player_transform), Ok(bonus_transform)) = (
                    transforms.q1().get(player_entity),
                    transforms.q1().get(bonus_entity),
                ) {
                    config.camera_settings.camera_should_focus = player_transform
                        .translation
                        .lerp(bonus_transform.translation, settings.lerp);
                }
            // otherwise, if there is only a player, target the player
            } else if let Some(player_entity) = config.target {
                if let Ok(player_transform) = transforms.q1().get(player_entity) {
                    config.camera_settings.camera_should_focus = player_transform.translation;
                }
            // otherwise, target the middle
            } else {
                config.camera_settings.camera_should_focus = Vec3::from(RESET_FOCUS);
            }
        }
        _ => {
            if let Some(player_entity) = config.target {
                if let Ok(player_transform) = transforms.q1().get(player_entity) {
                    match config.allowed_camera_modes.get(config.current_camera_mode).unwrap() {
                        CameraMode::Fps => {
                            delta_trans.translation = player_transform.translation;
                            settings.disable_move = true;

                            settings.disable_look = false;
                            delta_trans.rotation = player_transform.rotation;
                            delta_trans.translation += Vec3::new(/*-4.*/ 0., 1., 0.);
                        }
                        CameraMode::TopDown => {
                            delta_trans.translation = player_transform.translation;
                            settings.disable_move = true;

                            delta_trans.translation +=
                                Vec3::new(/*-4.*/ 0., settings.dist, 0.);
                            delta_trans.rotation =
                                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
                        }
                        CameraMode::TopDownDirection => {
                            settings.disable_move = true;
                            settings.locked_to_player = true;

                            delta_trans.translation +=
                                Vec3::new(/*-4.*/ 0., settings.dist, 0.);
                            delta_trans.rotation =
                                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                        }
                        CameraMode::FollowBehind => {
                            settings.disable_move = true;

                            settings.locked_to_player = true;

                            delta_trans.translation += Vec3::new(/*-4.*/ 0., 1., 4.);
                        }
                        _ => {}
                    }
                    config.camera_settings.camera_should_focus = player_transform.translation;
                }
            // otherwise, target the middle
            } else {
                config.camera_settings.camera_should_focus = Vec3::from(RESET_FOCUS);
            }
        }
    }

    const SPEED: f32 = 2.0;

    // calculate the camera motion based on the difference between where the camera is looking
    // and where it should be looking; the greater the distance, the faster the motion;
    // smooth out the camera movement using the frame time
    let mut camera_motion = config.camera_settings.camera_should_focus - config.camera_settings.camera_is_focus;
    if camera_motion.length() > 0.2 {
        camera_motion *= SPEED * time.delta_seconds();
        // set the new camera's actual focus
        config.camera_settings.camera_is_focus += camera_motion;
    }
    // look at that new camera's actual focus
    for (mut transform, camera) in transforms.q0_mut().iter_mut() {
        if camera.name == Some(CAMERA_3D.to_string()) {
            if delta_trans.translation != Vec3::ZERO {
                *transform = delta_trans
            } else {
                *transform = transform.looking_at(config.camera_settings.camera_is_focus, Vec3::Y)
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn toggle_camera_parent(
    mut act_cams: ResMut<ActiveCameras>,
    mut settings: ResMut<MovementSettings>,
    mut query: QuerySet<(
        Query<(&StaticCam, &mut Camera)>,
        Query<(&AttachedCam, &mut Camera)>,
    )>,
) {
    if settings.locked_to_player && !settings.ltp {
        act_cams.remove("Camera3d");

        let (_, mut b) = query.q1_mut().single_mut().unwrap();
        b.name = Some("Camera3d".to_string());

        act_cams.add("Camera3d");

        let (_, mut b) = query.q0_mut().single_mut().unwrap();
        b.name = Some("Test".to_string());

        settings.ltp = true;
    } else if !settings.locked_to_player && settings.ltp {
        act_cams.remove("Camera3d");

        let (_, mut b) = query.q0_mut().single_mut().unwrap();
        b.name = Some("Camera3d".to_string());

        act_cams.add("Camera3d");

        let (_, mut b) = query.q1_mut().single_mut().unwrap();
        b.name = Some("Test".to_string());

        settings.ltp = false;
    }
}

// Listens for Z key being pressed and toggles between the scroll-type states
#[allow(unused_must_use)]
fn switch_scroll_type(
    mut scroll_type: ResMut<State<ScrollType>>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::E) {
        let result = next_enum!(ScrollType, scroll_type);

        println!("{:?}", result);
        scroll_type.set(result);
    }
}

fn show_cams(
    mut query: Query<(&mut Camera, &mut PerspectiveProjection)>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::Y) {
        for (a,b) in query.iter_mut() {
            println!("{:?}", a);
        }
    }
}

// Depending on the state, the mouse-scroll changes either the movement speed or the field-of-view of the camera
fn scroll(
    mut settings: ResMut<MovementSettings>,
    mut p_settings: ResMut<PlayerSettings>,
    scroll_type: Res<State<ScrollType>>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    windows: Res<Windows>,
    mut query: Query<(&StaticCam, &mut Camera, &mut PerspectiveProjection)>,
) {
    for event in mouse_wheel_events.iter() {
        match *scroll_type.current() {
            ScrollType::MovementSpeed => {
                settings.speed = (settings.speed + event.y * 0.1).abs();
                println!("Speed: {:?}", settings.speed);
            }
            ScrollType::Sensitivity => {
                settings.sensitivity = (settings.sensitivity + event.y * 0.000001).abs();
                println!("Sensitivity: {:?}", settings.sensitivity);
            }
            ScrollType::Zoom => {
                for (_camera, mut camera, mut project) in query.iter_mut() {
                    project.fov = (project.fov - event.y * 0.01).abs();
                    let prim = windows.get_primary().unwrap();

                    //Calculate projection with new fov
                    project.update(prim.width(), prim.height());

                    //Update camera with the new fov
                    camera.projection_matrix = project.get_projection_matrix();
                    camera.depth_calculation = project.depth_calculation();

                    println!("FOV: {:?}", project.fov);
                }
            }
            ScrollType::Lerp => {
                settings.lerp = (settings.lerp + event.y * 0.01).abs();
                println!("Lerp: {:?}", settings.lerp);
            }
            ScrollType::CamFwd => {
                if event.y > 0.01 {
                    p_settings.cam_fwd = !p_settings.cam_fwd;
                }
                println!("CamFwd: {:?}", p_settings.cam_fwd);
            }
        }
    }
}

//bevy_flycam by sburris0 - https://github.com/sburris0/bevy_flycam

/// Keeps track of mouse motion events, pitch, and yaw
#[derive(Default)]
struct InputState {
    reader_motion: ManualEventReader<MouseMotion>,
    pitch: f32,
    yaw: f32,
}

/// Grabs/ungrabs mouse cursor
fn toggle_grab_cursor(window: &mut Window) {
    window.set_cursor_lock_mode(!window.cursor_locked());
    window.set_cursor_visibility(!window.cursor_visible());
}

/// Grabs the cursor when game first starts
fn initial_grab_cursor(mut windows: ResMut<Windows>) {
    toggle_grab_cursor(windows.get_primary_mut().unwrap());
}

pub fn validate_key<T>(codes: &'static [T], key: &T) -> bool
where
    T: PartialEq<T>,
{
    codes.iter().any(|m| m == key)
}

/// Handles looking around if cursor is locked
fn player_look(
    settings: Res<MovementSettings>,
    windows: Res<Windows>,
    mut state: ResMut<InputState>,
    motion: Res<Events<MouseMotion>>,
    mut query: Query<(&StaticCam, &mut Transform)>,
) {
    if settings.disable_look {
        return;
    }
    let window = windows.get_primary().unwrap();
    for (_camera, mut transform) in query.iter_mut() {
        for ev in state.reader_motion.iter(&motion) {
            if window.cursor_locked() {
                state.pitch -= (settings.sensitivity * ev.delta.y * window.height()).to_radians();
                state.yaw -= (settings.sensitivity * ev.delta.x * window.width()).to_radians();
            }

            state.pitch = state.pitch.clamp(-1.54, 1.54);

            // Order is important to prevent unintended roll
            transform.rotation = Quat::from_axis_angle(Vec3::Y, state.yaw)
                * Quat::from_axis_angle(Vec3::X, state.pitch);
        }
    }
}

fn cursor_grab(keys: Res<Input<KeyCode>>, mut windows: ResMut<Windows>) {
    let window = windows.get_primary_mut().unwrap();
    if keys.just_pressed(KeyCode::Escape) {
        toggle_grab_cursor(window);
    }
}

pub struct ConfigCam;
impl Plugin for ConfigCam {
    fn build(&self, app: &mut AppBuilder) {
        app .init_resource::<Config>()
            .add_plugin(NoCameraPlayerPlugin)
            .init_resource::<PlayerSettings>()
            .add_state(PluginState::Enabled)
            .add_state(ScrollType::MovementSpeed)
            .add_system(toggle_camera_parent.system())
            .add_system(switch_scroll_type.system())
            .add_system(scroll.system())
            .add_system(cycle_cam_state.system())
            .add_system_set(
                SystemSet::on_enter(PluginState::Enabled)
                .with_system(setup_camera.system())
                .with_system(setup_controller.system())
            )
            .add_system_set(
                SystemSet::on_update(PluginState::Enabled)
                    .with_system(move_player.system())
                    .with_system(move_camera.system())
            );
    }
}
/// Same as `PlayerPlugin` but does not spawn a camera
pub struct NoCameraPlayerPlugin;
impl Plugin for NoCameraPlayerPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.init_resource::<InputState>()
            .init_resource::<MovementSettings>()
            .add_startup_system(initial_grab_cursor.system())
            .add_system(player_move.system())
            .add_system(player_look.system())
            .add_system(cursor_grab.system());
    }
}
