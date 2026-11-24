package app.pley.android.designsystem.image

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/**
 * packages/placeholder/placeholder.json, which every app draws its
 * placeholder from, found from the module's folder up to the repository's
 * root.
 */
internal val placeholderFile: JsonObject by lazy {
    val here = File("").absoluteFile
    val file = generateSequence(here) { it.parentFile }
        .map { File(it, "packages/placeholder/placeholder.json") }
        .firstOrNull { it.isFile }
        ?: error("no packages/placeholder/placeholder.json above $here")
    Json.parseToJsonElement(file.readText()).jsonObject
}
