
// Bake mode's 3D view: the high poly, the cage around the low poly and the UV seams

struct BakeViewOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) wpos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    // 1 where the bake's rays miss the high poly, 0 where they reach it
    @location(2) missed: f32,
};

const HIGH_POLY_COLOR: vec3<f32> = vec3<f32>(0.10, 0.36, 0.56);
const MISSED_COLOR: vec3<f32> = vec3<f32>(0.80, 0.01, 0.01);
const CAGE_COLOR: vec3<f32> = vec3<f32>(0.55, 0.42, 0.26);
const CAGE_OPACITY: f32 = 0.16;
const CAGE_WIRE_COLOR: vec4<f32> = vec4<f32>(0.75, 0.62, 0.42, 0.7);
const UV_SEAM_COLOR: vec4<f32> = vec4<f32>(0.20, 0.85, 0.30, 1.0);

// Lit from the camera: surfaces facing it are brightest
fn headlight(color: vec3<f32>, normal: vec3<f32>, wpos: vec3<f32>) -> vec3<f32> {
    let facing = abs(dot(normal, normalize(g.eye.xyz - wpos)));
    return color * (0.3 + 0.7 * facing);
}

// Pulled slightly toward the camera: where the high poly nearly touches the low poly it shows instead of flickering with it
@vertex
fn vs_bake_high(@location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(4) missed: f32) -> BakeViewOut {
    var o: BakeViewOut;
    o.clip = g.view_proj * vec4<f32>(pos + (g.eye.xyz - pos) * 0.001, 1.0);
    o.wpos = pos;
    o.nrm = nrm;
    o.missed = missed;
    return o;
}

@fragment
fn fs_bake_high(i: BakeViewOut) -> @location(0) vec4<f32> {
    let color = mix(HIGH_POLY_COLOR, MISSED_COLOR, i.missed);
    return vec4<f32>(headlight(color, normalize(i.nrm), i.wpos), 1.0);
}

@vertex
fn vs_bake_cage(@location(0) pos: vec3<f32>) -> BakeViewOut {
    var o: BakeViewOut;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.wpos = pos;
    o.nrm = vec3<f32>(0.0, 1.0, 0.0);
    o.missed = 0.0;
    return o;
}

// The cage's own faces: their normal comes from the screen-space change of the position
@fragment
fn fs_bake_cage(i: BakeViewOut) -> @location(0) vec4<f32> {
    let face_normal = normalize(cross(dpdx(i.wpos), dpdy(i.wpos)));
    return vec4<f32>(headlight(CAGE_COLOR, face_normal, i.wpos), CAGE_OPACITY);
}

// Pulled slightly toward the camera like the 3D wireframe, so the lines win against the cage's faces
@vertex
fn vs_bake_cage_wire(@location(0) pos: vec3<f32>) -> BakeViewOut {
    var o: BakeViewOut;
    o.clip = g.view_proj * vec4<f32>(pos + (g.eye.xyz - pos) * 0.002, 1.0);
    o.wpos = pos;
    o.nrm = vec3<f32>(0.0, 1.0, 0.0);
    o.missed = 0.0;
    return o;
}

@fragment
fn fs_wire_bake_cage(i: BakeViewOut) -> @location(0) vec4<f32> {
    return CAGE_WIRE_COLOR;
}

@fragment
fn fs_wire_uv_seam(i: VOut) -> @location(0) vec4<f32> {
    return UV_SEAM_COLOR;
}
