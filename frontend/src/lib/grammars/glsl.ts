import type { LanguageRegistration } from "shiki/core";

/**
 * Browsitory's own minimal GLSL grammar (#326), MIT like the rest of Browsitory.
 *
 * Shiki's C++ grammar highlights the GLSL inside `R"glsl(...)glsl"` raw strings by including the
 * scope `source.glsl`. Shiki's own `glsl` grammar (polym0rph/GLSL.tmbundle) has no licence, so
 * it is excluded (`excludedGrammars.ts`) and this one is registered under its name instead
 * (owner decision 2026-11-12). GLSL files themselves aren't highlighted; this only has to serve
 * that embed.
 *
 * Written for Browsitory, not derived from another grammar: the words below are the reserved
 * keywords, qualifiers, types and built-in names of the OpenGL Shading Language specification
 * (Khronos, version 4.60). Everything else (comments, numbers, operators, preprocessor lines,
 * function calls) comes from the C grammar, included last.
 */
const words = (list: string) => `\\b(?:${list.trim().split(/\s+/).join("|")})\\b`;

export const GLSL_GRAMMAR: LanguageRegistration = {
  name: "glsl",
  displayName: "GLSL",
  scopeName: "source.glsl",
  embeddedLangs: ["c"],
  repository: {
    // `#version` and `#extension` are GLSL's; C knows the other directives.
    directive: {
      match: "^\\s*(#)\\s*(version|extension)\\b",
      captures: {
        1: { name: "punctuation.definition.directive.glsl" },
        2: { name: "keyword.control.directive.glsl" },
      },
    },
    control: {
      match: words("break continue do for while switch case default if else discard return demote"),
      name: "keyword.control.glsl",
    },
    qualifier: {
      match: words(`
        const in out inout uniform buffer shared attribute varying layout
        centroid flat smooth noperspective patch sample invariant precise
        coherent volatile restrict readonly writeonly subroutine
        highp mediump lowp precision`),
      name: "storage.modifier.glsl",
    },
    type: {
      patterns: [
        { match: words("void bool int uint float double struct atomic_uint sampler samplerShadow"), name: "storage.type.glsl" },
        // vec2..4 and their b/i/u/d variants; mat2..4, matNxM and their d variants.
        { match: "\\b[biud]?vec[234]\\b", name: "storage.type.glsl" },
        { match: "\\bd?mat[234](?:x[234])?\\b", name: "storage.type.glsl" },
        // Opaque types: sampler2D, isampler2DArray, usamplerCube, image3D, texture2DMS, ….
        {
          match: "\\b[iu]?(?:sampler|image|texture)(?:1D|2D|3D|Cube|2DRect|Buffer)(?:MS)?(?:Array)?(?:Shadow)?\\b",
          name: "storage.type.glsl",
        },
        { match: "\\b[iu]?subpassInput(?:MS)?\\b", name: "storage.type.glsl" },
      ],
    },
    // Every built-in variable and constant starts with `gl_`.
    "builtin-variable": { match: "\\bgl_[A-Za-z0-9_]+\\b", name: "variable.language.glsl" },
    "builtin-function": {
      match: words(`
        radians degrees sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh
        pow exp log exp2 log2 sqrt inversesqrt
        abs sign floor trunc round roundEven ceil fract mod modf min max clamp mix step
        smoothstep isnan isinf fma
        length distance dot cross normalize faceforward reflect refract
        matrixCompMult outerProduct transpose determinant inverse
        lessThan lessThanEqual greaterThan greaterThanEqual equal notEqual any all not
        texture textureSize textureProj textureLod textureOffset texelFetch texelFetchOffset
        textureProjOffset textureLodOffset textureProjLod textureGrad textureGather
        imageLoad imageStore imageSize
        dFdx dFdy fwidth
        barrier memoryBarrier groupMemoryBarrier
        EmitVertex EndPrimitive`),
      name: "support.function.glsl",
    },
  },
  patterns: [
    { include: "#directive" },
    { include: "#control" },
    { include: "#qualifier" },
    { include: "#type" },
    { include: "#builtin-variable" },
    { include: "#builtin-function" },
    { include: "source.c" },
  ],
};
