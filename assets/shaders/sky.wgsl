// Since post processing is a fullscreen effect, we use the fullscreen vertex shader provided by bevy.
// This will import a vertex shader that renders a single fullscreen triangle.
//
// A fullscreen triangle is a single triangle that covers the entire screen.
// The box in the top left in that diagram is the screen. The 4 x are the corner of the screen
//
// Y axis
//  1 |  x-----x......
//  0 |  |  s  |  . ´
// -1 |  x_____x´
// -2 |  :  .´
// -3 |  :´
//    +---------------  X axis
//      -1  0  1  2  3
//
// As you can see, the triangle ends up bigger than the screen.
//
// You don't need to worry about this too much since bevy will compute the correct UVs for you.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
struct SkySettings {
   time_of_day: f32,
   sun_rotation: vec4<f32>,
   moon_rotation: vec4<f32>,
}
@group(0) @binding(2) var<uniform> settings: SkySettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(screen_texture, texture_sampler, in.uv);
    if (color.a == 1.) {
        return color;
    }

    let light_blue = vec3(0.341, 0.725, 1.);
    let light_blue_srgb = pow(light_blue, vec3(2.2));

    return mix(vec4(light_blue_srgb, 1.), color, color.a);
}
