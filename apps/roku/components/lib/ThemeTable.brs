' Pley's themes and accents as packages/themes/themes.json has them,
' written by scripts/themes.mjs (bun run themes): tests/themes.test.brs
' fails while this differs from the JSON. Colors are Roku's without their
' 0x (RRGGBBAA), which lib/Themes.brs adds; tokens are camelCase.

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
                accent: "38BDF8FF",
                accentHover: "7DD3FCFF",
                accentFg: "03131FFF",
                brandLight: "BAE6FDFF",
                brand: "38BDF8FF",
                brandDeep: "0284C7FF",
                brandFar: "0C4A6EFF",
                brandTile: "0A1622FF",
                bg: "0B0D10FF",
                surface: "12151AFF",
                surface2: "191D24FF",
                surface3: "222731FF",
                border: "2A303BFF",
                borderStrong: "3A4250FF",
                fg: "E9EBEFFF",
                muted: "A1A9B6FF",
                subtle: "7E8795FF",
                success: "3FB27FFF",
                danger: "ED6C6DFF",
                dangerFg: "0B0D10FF",
                info: "4C9AFFFF",
                warning: "F0A83CFF",
                shade: "000000FF"
            }
        },
        midday: {
            kind: "light",
            twin: "midnight",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "03669CFF",
                accentHover: "075985FF",
                accentFg: "FFFFFFFF",
                brandLight: "38BDF8FF",
                brand: "0369A1FF",
                brandDeep: "3D9FD8FF",
                brandFar: "9AC9EDFF",
                brandTile: "DDEEFAFF",
                bg: "F3F5F8FF",
                surface: "FFFFFFFF",
                surface2: "EEF1F5FF",
                surface3: "E3E7EDFF",
                border: "D5DAE2FF",
                borderStrong: "BCC4CFFF",
                fg: "12161CFF",
                muted: "4A5361FF",
                subtle: "5D6675FF",
                success: "126E46FF",
                danger: "B42D36FF",
                dangerFg: "FFFFFFFF",
                info: "1E5DC0FF",
                warning: "855700FF",
                shade: "101A2C6B"
            }
        },
        twilight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "FF79B0FF",
                accentHover: "FFA0C8FF",
                accentFg: "2A0619FF",
                brandLight: "FFD3A6FF",
                brand: "FF79B0FF",
                brandDeep: "C2448FFF",
                brandFar: "6E2C7AFF",
                brandTile: "1F1233FF",
                bg: "140E22FF",
                surface: "1C142EFF",
                surface2: "261B3CFF",
                surface3: "31244BFF",
                border: "3A2B55FF",
                borderStrong: "4F3B72FF",
                fg: "F8EEF6FF",
                muted: "C9B8D1FF",
                subtle: "9A88ABFF",
                success: "4CC38AFF",
                danger: "F0727EFF",
                dangerFg: "140E22FF",
                info: "7AA7FFFF",
                warning: "FFA24CFF",
                shade: "000000FF"
            }
        },
        starlight: {
            kind: "light",
            twin: "moonlight",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "835613FF",
                accentHover: "774E11FF",
                accentFg: "FFFAF2FF",
                brandLight: "D6A548FF",
                brand: "8A5A12FF",
                brandDeep: "C0904AFF",
                brandFar: "DCC08AFF",
                brandTile: "EFE3C8FF",
                bg: "F6F0E5FF",
                surface: "FDFAF4FF",
                surface2: "F1E9DBFF",
                surface3: "E8DFCDFF",
                border: "DDD2BDFF",
                borderStrong: "C6B79CFF",
                fg: "2A2318FF",
                muted: "5C5141FF",
                subtle: "685D4AFF",
                success: "206C45FF",
                danger: "AC2F28FF",
                dangerFg: "FFFFFFFF",
                info: "2B5DAFFF",
                warning: "8A5200FF",
                shade: "422C0C61"
            }
        },
        moonlight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "F5C542FF",
                accentHover: "F9D97CFF",
                accentFg: "1C1604FF",
                brandLight: "FFF3C9FF",
                brand: "F5C542FF",
                brandDeep: "B48D2BFF",
                brandFar: "66532AFF",
                brandTile: "18191BFF",
                bg: "0C0D0EFF",
                surface: "141517FF",
                surface2: "1C1E20FF",
                surface3: "25272AFF",
                border: "2E3033FF",
                borderStrong: "404347FF",
                fg: "F3EAD9FF",
                muted: "BDB6A8FF",
                subtle: "8E8A82FF",
                success: "5BC28CFF",
                danger: "E8726DFF",
                dangerFg: "0C0D0EFF",
                info: "6FA8FFFF",
                warning: "F59A45FF",
                shade: "000000FF"
            }
        },
        nightshade: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "5FD39AFF",
                accentHover: "8FE3B8FF",
                accentFg: "03140AFF",
                brandLight: "C9F6DCFF",
                brand: "5FD39AFF",
                brandDeep: "2F9A68FF",
                brandFar: "1D5A3EFF",
                brandTile: "0F1A14FF",
                bg: "0A0F0CFF",
                surface: "111814FF",
                surface2: "18211CFF",
                surface3: "212C25FF",
                border: "29352EFF",
                borderStrong: "3A4A41FF",
                fg: "E7EFE9FF",
                muted: "A2B3A8FF",
                subtle: "798C81FF",
                success: "3CC2A6FF",
                danger: "E8726DFF",
                dangerFg: "0A0F0CFF",
                info: "6AA6FFFF",
                warning: "F0A83CFF",
                shade: "000000FF"
            }
        },
        greenlight: {
            kind: "light",
            twin: "nightshade",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar", "success", "warning"],
            colors: {
                accent: "1A6E45FF",
                accentHover: "16623EFF",
                accentFg: "FFFFFFFF",
                brandLight: "5FD39AFF",
                brand: "1A7048FF",
                brandDeep: "4FAE80FF",
                brandFar: "98CDB0FF",
                brandTile: "DBEFE2FF",
                bg: "F1F6F2FF",
                surface: "FBFDFBFF",
                surface2: "E8F0EAFF",
                surface3: "DCE7DFFF",
                border: "CDDBD1FF",
                borderStrong: "B2C4B7FF",
                fg: "122017FF",
                muted: "45564AFF",
                subtle: "58695DFF",
                success: "0D6A60FF",
                danger: "B02C35FF",
                dangerFg: "FFFFFFFF",
                info: "1E5DC0FF",
                warning: "855700FF",
                shade: "0E2C1A61"
            }
        },
        sunshine: {
            kind: "light",
            twin: "midnight",
            follows: ["accent", "accentHover", "accentFg", "success", "warning"],
            colors: {
                accent: "5A2FF0FF",
                accentHover: "4A1FD6FF",
                accentFg: "FFFFFFFF",
                brandLight: "FFD84AFF",
                brand: "FF8A1FFF",
                brandDeep: "FF5FA2FF",
                brandFar: "3FC1FFFF",
                brandTile: "3B2A7AFF",
                bg: "FFF7E0FF",
                surface: "FFFFFFFF",
                surface2: "E8F4FFFF",
                surface3: "EFE7FFFF",
                border: "DDD3F6FF",
                borderStrong: "C3B4F0FF",
                fg: "24124FFF",
                muted: "4F437CFF",
                subtle: "62578FFF",
                success: "0C713DFF",
                danger: "B81E3FFF",
                dangerFg: "FFFFFFFF",
                info: "135EC2FF",
                warning: "A14700FF",
                shade: "3A1E6457"
            }
        },
        blacklight: {
            kind: "dark",
            twin: "",
            follows: ["accent", "accentHover", "accentFg", "brandLight", "brand", "brandDeep", "brandFar"],
            colors: {
                accent: "FFFFFFFF",
                accentHover: "D6D6D6FF",
                accentFg: "000000FF",
                brandLight: "FFFFFFFF",
                brand: "BDBDBDFF",
                brandDeep: "8A8A8AFF",
                brandFar: "4D4D4DFF",
                brandTile: "141414FF",
                bg: "000000FF",
                surface: "0D0D0DFF",
                surface2: "171717FF",
                surface3: "222222FF",
                border: "2E2E2EFF",
                borderStrong: "4A4A4AFF",
                fg: "FFFFFFFF",
                muted: "B5B5B5FF",
                subtle: "8E8E8EFF",
                success: "E0E0E0FF",
                danger: "8F8F8FFF",
                dangerFg: "000000FF",
                info: "C4C4C4FF",
                warning: "A6A6A6FF",
                shade: "000000FF"
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
                accent: "38BDF8FF",
                accentHover: "7DD3FCFF",
                accentFg: "03131FFF",
                brandLight: "BAE6FDFF",
                brand: "38BDF8FF",
                brandDeep: "0284C7FF",
                brandFar: "0C4A6EFF"
            },
            light: {
                accent: "025D8FFF",
                accentHover: "034E78FF",
                accentFg: "FFFFFFFF",
                brandLight: "38BDF8FF",
                brand: "0369A1FF",
                brandDeep: "3D9FD8FF",
                brandFar: "9AC9EDFF"
            }
        },
        violet: {
            dark: {
                accent: "B39AFDFF",
                accentHover: "CDBFFEFF",
                accentFg: "160B33FF",
                brandLight: "E2D9FFFF",
                brand: "B39AFDFF",
                brandDeep: "7B5CE6FF",
                brandFar: "43307FFF"
            },
            light: {
                accent: "5A2FF0FF",
                accentHover: "4A1FD6FF",
                accentFg: "FFFFFFFF",
                brandLight: "B39AFDFF",
                brand: "5A2FF0FF",
                brandDeep: "8B6CF6FF",
                brandFar: "C6B6F8FF"
            }
        },
        miami: {
            dark: {
                accent: "FF79B0FF",
                accentHover: "FFA0C8FF",
                accentFg: "2A0619FF",
                brandLight: "FFD3A6FF",
                brand: "FF79B0FF",
                brandDeep: "C2448FFF",
                brandFar: "6E2C7AFF"
            },
            light: {
                accent: "A61E6AFF",
                accentHover: "901A5CFF",
                accentFg: "FFFFFFFF",
                brandLight: "FF79B0FF",
                brand: "A61E6AFF",
                brandDeep: "DB5B9BFF",
                brandFar: "EEB4CFFF"
            }
        },
        gold: {
            dark: {
                accent: "F5C542FF",
                accentHover: "F9D97CFF",
                accentFg: "1C1604FF",
                brandLight: "FFF3C9FF",
                brand: "F5C542FF",
                brandDeep: "B48D2BFF",
                brandFar: "66532AFF",
                warning: "FF8F5AFF"
            },
            light: {
                accent: "835613FF",
                accentHover: "774E11FF",
                accentFg: "FFFAF2FF",
                brandLight: "D6A548FF",
                brand: "8A5A12FF",
                brandDeep: "C0904AFF",
                brandFar: "DCC08AFF",
                warning: "9A3F17FF"
            }
        },
        mint: {
            dark: {
                accent: "5FD39AFF",
                accentHover: "8FE3B8FF",
                accentFg: "03140AFF",
                brandLight: "C9F6DCFF",
                brand: "5FD39AFF",
                brandDeep: "2F9A68FF",
                brandFar: "1D5A3EFF",
                success: "3CC2A6FF"
            },
            light: {
                accent: "18663FFF",
                accentHover: "145A37FF",
                accentFg: "FFFFFFFF",
                brandLight: "5FD39AFF",
                brand: "1A7048FF",
                brandDeep: "4FAE80FF",
                brandFar: "98CDB0FF",
                success: "0D6A60FF"
            }
        },
        amber: {
            dark: {
                accent: "FF6F3CFF",
                accentHover: "FF956CFF",
                accentFg: "1F0702FF",
                brandLight: "FFCBB0FF",
                brand: "FF6F3CFF",
                brandDeep: "C4461AFF",
                brandFar: "6B2612FF",
                warning: "E8C23AFF"
            },
            light: {
                accent: "A03D00FF",
                accentHover: "8B3500FF",
                accentFg: "FFFFFFFF",
                brandLight: "FF6F3CFF",
                brand: "A03D00FF",
                brandDeep: "DE5A2CFF",
                brandFar: "EFAE92FF",
                warning: "6F5A00FF"
            }
        }
    }
end function
