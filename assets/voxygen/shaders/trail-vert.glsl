#version 440 core

#include <globals.glsl>
#include <cube.glsl>

layout(location = 0) in vec3 v_pos;

void main() {
    vec3 f_pos = v_pos - focus_off.xyz;

    // Les traînées — bouts d'aile du planeur, armes, projectiles — sont un
    // ruban de positions **absolues** du monde, recalculé chaque image par le
    // CPU ; ce shader n'en était qu'un passe-plat, donc elles restaient à plat.
    //
    // Chaque sommet est sa propre ancre : un ruban est mince, il n'y a pas de
    // corps rigide à préserver, et `cube_poser` rend alors simplement la
    // projection du point.
    if (cube_actif()) {
        f_pos = cube_poser(v_pos, f_pos);
    }

    gl_Position = all_mat * vec4(f_pos, 1);
}
