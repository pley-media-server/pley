' Pley's themes and accents as packages/themes/themes.json has them,
' written by scripts/themes.mjs (bun run themes): tests/themes.test.brs
' fails while this differs from the JSON. Colors are Roku's, the JSON's
' own; tokens are camelCase.

' The themes' ids, in the picker's order.
function themeIds() as object
    return ["midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight"]
end function

' The accents' ids, in the picker's order (the theme's own before them).
function themeAccentIds() as object
    return ["sky", "violet", "miami", "gold", "mint", "amber"]
end function

' Each theme: dark or light, the dark theme its players draw in ("" for a
' dark one), the tokens an accent replaces and its own colors.
function themeTable() as object
    return {
        midnight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0x38BDF8FF",
                accentHover: "0x7DD3FCFF",
                accentFg: "0x03131FFF",
                brandLight: "0xBAE6FDFF",
                brand: "0x38BDF8FF",
                brandDeep: "0x0284C7FF",
                brandFar: "0x0C4A6EFF",
                brandTile: "0x0A1622FF",
                bg: "0x0B0D10FF",
                surface: "0x12151AFF",
                surface2: "0x191D24FF",
                surface3: "0x222731FF",
                border: "0x2A303BFF",
                borderStrong: "0x3A4250FF",
                fg: "0xE9EBEFFF",
                muted: "0xA1A9B6FF",
                subtle: "0x7E8795FF",
                success: "0x3FB27FFF",
                danger: "0xED6C6DFF",
                dangerFg: "0x0B0D10FF",
                info: "0x4C9AFFFF",
                warning: "0xF0A83CFF",
                shade: "0x000000FF"
            }
        },
        midday: {
            kind: "light",
            twin: "midnight",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0x03669CFF",
                accentHover: "0x075985FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0x38BDF8FF",
                brand: "0x0369A1FF",
                brandDeep: "0x3D9FD8FF",
                brandFar: "0x9AC9EDFF",
                brandTile: "0xDDEEFAFF",
                bg: "0xF3F5F8FF",
                surface: "0xFFFFFFFF",
                surface2: "0xEEF1F5FF",
                surface3: "0xE3E7EDFF",
                border: "0xD5DAE2FF",
                borderStrong: "0xBCC4CFFF",
                fg: "0x12161CFF",
                muted: "0x4A5361FF",
                subtle: "0x5D6675FF",
                success: "0x126E46FF",
                danger: "0xB42D36FF",
                dangerFg: "0xFFFFFFFF",
                info: "0x1E5DC0FF",
                warning: "0x855700FF",
                shade: "0x101A2C6B"
            }
        },
        twilight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0xFF79B0FF",
                accentHover: "0xFFA0C8FF",
                accentFg: "0x2A0619FF",
                brandLight: "0xFFD3A6FF",
                brand: "0xFF79B0FF",
                brandDeep: "0xC2448FFF",
                brandFar: "0x6E2C7AFF",
                brandTile: "0x1F1233FF",
                bg: "0x140E22FF",
                surface: "0x1C142EFF",
                surface2: "0x261B3CFF",
                surface3: "0x31244BFF",
                border: "0x3A2B55FF",
                borderStrong: "0x4F3B72FF",
                fg: "0xF8EEF6FF",
                muted: "0xC9B8D1FF",
                subtle: "0x9A88ABFF",
                success: "0x4CC38AFF",
                danger: "0xF0727EFF",
                dangerFg: "0x140E22FF",
                info: "0x7AA7FFFF",
                warning: "0xFFA24CFF",
                shade: "0x000000FF"
            }
        },
        starlight: {
            kind: "light",
            twin: "moonlight",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0x835613FF",
                accentHover: "0x774E11FF",
                accentFg: "0xFFFAF2FF",
                brandLight: "0xD6A548FF",
                brand: "0x8A5A12FF",
                brandDeep: "0xC0904AFF",
                brandFar: "0xDCC08AFF",
                brandTile: "0xEFE3C8FF",
                bg: "0xF6F0E5FF",
                surface: "0xFDFAF4FF",
                surface2: "0xF1E9DBFF",
                surface3: "0xE8DFCDFF",
                border: "0xDDD2BDFF",
                borderStrong: "0xC6B79CFF",
                fg: "0x2A2318FF",
                muted: "0x5C5141FF",
                subtle: "0x685D4AFF",
                success: "0x206C45FF",
                danger: "0xAC2F28FF",
                dangerFg: "0xFFFFFFFF",
                info: "0x2B5DAFFF",
                warning: "0x8A5200FF",
                shade: "0x422C0C61"
            }
        },
        moonlight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0xF5C542FF",
                accentHover: "0xF9D97CFF",
                accentFg: "0x1C1604FF",
                brandLight: "0xFFF3C9FF",
                brand: "0xF5C542FF",
                brandDeep: "0xB48D2BFF",
                brandFar: "0x66532AFF",
                brandTile: "0x18191BFF",
                bg: "0x0C0D0EFF",
                surface: "0x141517FF",
                surface2: "0x1C1E20FF",
                surface3: "0x25272AFF",
                border: "0x2E3033FF",
                borderStrong: "0x404347FF",
                fg: "0xF3EAD9FF",
                muted: "0xBDB6A8FF",
                subtle: "0x8E8A82FF",
                success: "0x5BC28CFF",
                danger: "0xE8726DFF",
                dangerFg: "0x0C0D0EFF",
                info: "0x6FA8FFFF",
                warning: "0xF59A45FF",
                shade: "0x000000FF"
            }
        },
        nightshade: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0x5FD39AFF",
                accentHover: "0x8FE3B8FF",
                accentFg: "0x03140AFF",
                brandLight: "0xC9F6DCFF",
                brand: "0x5FD39AFF",
                brandDeep: "0x2F9A68FF",
                brandFar: "0x1D5A3EFF",
                brandTile: "0x0F1A14FF",
                bg: "0x0A0F0CFF",
                surface: "0x111814FF",
                surface2: "0x18211CFF",
                surface3: "0x212C25FF",
                border: "0x29352EFF",
                borderStrong: "0x3A4A41FF",
                fg: "0xE7EFE9FF",
                muted: "0xA2B3A8FF",
                subtle: "0x798C81FF",
                success: "0x3CC2A6FF",
                danger: "0xE8726DFF",
                dangerFg: "0x0A0F0CFF",
                info: "0x6AA6FFFF",
                warning: "0xF0A83CFF",
                shade: "0x000000FF"
            }
        },
        greenlight: {
            kind: "light",
            twin: "nightshade",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "0x1A6E45FF",
                accentHover: "0x16623EFF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0x5FD39AFF",
                brand: "0x1A7048FF",
                brandDeep: "0x4FAE80FF",
                brandFar: "0x98CDB0FF",
                brandTile: "0xDBEFE2FF",
                bg: "0xF1F6F2FF",
                surface: "0xFBFDFBFF",
                surface2: "0xE8F0EAFF",
                surface3: "0xDCE7DFFF",
                border: "0xCDDBD1FF",
                borderStrong: "0xB2C4B7FF",
                fg: "0x122017FF",
                muted: "0x45564AFF",
                subtle: "0x58695DFF",
                success: "0x0D6A60FF",
                danger: "0xB02C35FF",
                dangerFg: "0xFFFFFFFF",
                info: "0x1E5DC0FF",
                warning: "0x855700FF",
                shade: "0x0E2C1A61"
            }
        },
        sunshine: {
            kind: "light",
            twin: "midnight",
            follows: ["accent", "accentHover", "accentFg", "success", "warning"],
            colors: {
                accent: "0x5A2FF0FF",
                accentHover: "0x4A1FD6FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0xFFD84AFF",
                brand: "0xFF8A1FFF",
                brandDeep: "0xFF5FA2FF",
                brandFar: "0x3FC1FFFF",
                brandTile: "0x3B2A7AFF",
                bg: "0xFFF7E0FF",
                surface: "0xFFFFFFFF",
                surface2: "0xE8F4FFFF",
                surface3: "0xEFE7FFFF",
                border: "0xDDD3F6FF",
                borderStrong: "0xC3B4F0FF",
                fg: "0x24124FFF",
                muted: "0x4F437CFF",
                subtle: "0x62578FFF",
                success: "0x0C713DFF",
                danger: "0xB81E3FFF",
                dangerFg: "0xFFFFFFFF",
                info: "0x135EC2FF",
                warning: "0xA14700FF",
                shade: "0x3A1E6457"
            }
        },
        blacklight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar"],
            colors: {
                accent: "0xFFFFFFFF",
                accentHover: "0xD6D6D6FF",
                accentFg: "0x000000FF",
                brandLight: "0xFFFFFFFF",
                brand: "0xBDBDBDFF",
                brandDeep: "0x8A8A8AFF",
                brandFar: "0x4D4D4DFF",
                brandTile: "0x141414FF",
                bg: "0x000000FF",
                surface: "0x0D0D0DFF",
                surface2: "0x171717FF",
                surface3: "0x222222FF",
                border: "0x2E2E2EFF",
                borderStrong: "0x4A4A4AFF",
                fg: "0xFFFFFFFF",
                muted: "0xB5B5B5FF",
                subtle: "0x8E8E8EFF",
                success: "0xE0E0E0FF",
                danger: "0x8F8F8FFF",
                dangerFg: "0x000000FF",
                info: "0xC4C4C4FF",
                warning: "0xA6A6A6FF",
                shade: "0x000000FF"
            }
        }
    }
end function

' Each accent: the colors it gives the tokens a theme follows, for the dark
' themes and for the light ones.
function themeAccentTable() as object
    return {
        sky: {
            dark: {
                accent: "0x38BDF8FF",
                accentHover: "0x7DD3FCFF",
                accentFg: "0x03131FFF",
                brandLight: "0xBAE6FDFF",
                brand: "0x38BDF8FF",
                brandDeep: "0x0284C7FF",
                brandFar: "0x0C4A6EFF"
            },
            light: {
                accent: "0x025D8FFF",
                accentHover: "0x034E78FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0x38BDF8FF",
                brand: "0x0369A1FF",
                brandDeep: "0x3D9FD8FF",
                brandFar: "0x9AC9EDFF"
            }
        },
        violet: {
            dark: {
                accent: "0xB39AFDFF",
                accentHover: "0xCDBFFEFF",
                accentFg: "0x160B33FF",
                brandLight: "0xE2D9FFFF",
                brand: "0xB39AFDFF",
                brandDeep: "0x7B5CE6FF",
                brandFar: "0x43307FFF"
            },
            light: {
                accent: "0x5A2FF0FF",
                accentHover: "0x4A1FD6FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0xB39AFDFF",
                brand: "0x5A2FF0FF",
                brandDeep: "0x8B6CF6FF",
                brandFar: "0xC6B6F8FF"
            }
        },
        miami: {
            dark: {
                accent: "0xFF79B0FF",
                accentHover: "0xFFA0C8FF",
                accentFg: "0x2A0619FF",
                brandLight: "0xFFD3A6FF",
                brand: "0xFF79B0FF",
                brandDeep: "0xC2448FFF",
                brandFar: "0x6E2C7AFF"
            },
            light: {
                accent: "0xA61E6AFF",
                accentHover: "0x901A5CFF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0xFF79B0FF",
                brand: "0xA61E6AFF",
                brandDeep: "0xDB5B9BFF",
                brandFar: "0xEEB4CFFF"
            }
        },
        gold: {
            dark: {
                accent: "0xF5C542FF",
                accentHover: "0xF9D97CFF",
                accentFg: "0x1C1604FF",
                brandLight: "0xFFF3C9FF",
                brand: "0xF5C542FF",
                brandDeep: "0xB48D2BFF",
                brandFar: "0x66532AFF",
                warning: "0xFF8F5AFF"
            },
            light: {
                accent: "0x835613FF",
                accentHover: "0x774E11FF",
                accentFg: "0xFFFAF2FF",
                brandLight: "0xD6A548FF",
                brand: "0x8A5A12FF",
                brandDeep: "0xC0904AFF",
                brandFar: "0xDCC08AFF",
                warning: "0x9A3F17FF"
            }
        },
        mint: {
            dark: {
                accent: "0x5FD39AFF",
                accentHover: "0x8FE3B8FF",
                accentFg: "0x03140AFF",
                brandLight: "0xC9F6DCFF",
                brand: "0x5FD39AFF",
                brandDeep: "0x2F9A68FF",
                brandFar: "0x1D5A3EFF",
                success: "0x3CC2A6FF"
            },
            light: {
                accent: "0x18663FFF",
                accentHover: "0x145A37FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0x5FD39AFF",
                brand: "0x1A7048FF",
                brandDeep: "0x4FAE80FF",
                brandFar: "0x98CDB0FF",
                success: "0x0D6A60FF"
            }
        },
        amber: {
            dark: {
                accent: "0xFF6F3CFF",
                accentHover: "0xFF956CFF",
                accentFg: "0x1F0702FF",
                brandLight: "0xFFCBB0FF",
                brand: "0xFF6F3CFF",
                brandDeep: "0xC4461AFF",
                brandFar: "0x6B2612FF",
                warning: "0xE8C23AFF"
            },
            light: {
                accent: "0xA03D00FF",
                accentHover: "0x8B3500FF",
                accentFg: "0xFFFFFFFF",
                brandLight: "0xFF6F3CFF",
                brand: "0xA03D00FF",
                brandDeep: "0xDE5A2CFF",
                brandFar: "0xEFAE92FF",
                warning: "0x6F5A00FF"
            }
        }
    }
end function
