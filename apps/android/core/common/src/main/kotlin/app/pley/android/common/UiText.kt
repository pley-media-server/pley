package app.pley.android.common

import android.content.Context
import android.content.res.Resources
import androidx.annotation.PluralsRes
import androidx.annotation.StringRes

/**
 * Words for people made where nothing is drawn (a ViewModel, a repository,
 * an error), put into the language shown where they show: [resolve], or
 * `asString()` in Compose (:core:designsystem). A ViewModel holds these,
 * never the words themselves, so a change of language reaches what it
 * shows. An argument that is a UiText itself is resolved first.
 */
sealed interface UiText {
    /** A string resource, and what goes in its `%1$s`s. */
    data class Res(@StringRes val id: Int, val args: List<Any> = emptyList()) : UiText

    /** A plurals resource for [count], with [args] in its `%1$d`s: the count itself unless others are given. */
    data class Plural(@PluralsRes val id: Int, val count: Int, val args: List<Any> = listOf(count)) : UiText

    /** Words as they are, in no language of the app's: the server's, a name, what someone typed. */
    data class Raw(val text: String) : UiText

    /** [text] as a sentence: a capital first, a full stop last ([asSentence]). */
    data class Sentence(val text: UiText) : UiText

    companion object {
        fun res(@StringRes id: Int, vararg args: Any): UiText = Res(id, args.toList())

        fun plural(@PluralsRes id: Int, count: Int, vararg args: Any): UiText =
            Plural(id, count, if (args.isEmpty()) listOf(count) else args.toList())

        fun raw(text: String): UiText = Raw(text)
    }
}

/** These words in the language of [resources]. */
fun UiText.resolve(resources: Resources): String = when (this) {
    is UiText.Res -> if (args.isEmpty()) resources.getString(id) else resources.getString(id, *resolved(args, resources))
    is UiText.Plural -> resources.getQuantityString(id, count, *resolved(args, resources))
    is UiText.Raw -> text
    is UiText.Sentence -> asSentence(text.resolve(resources))
}

/** These words in the language of [context]: an activity's or a service's, made in the language shown. */
fun UiText.resolve(context: Context): String = resolve(context.resources)

private fun resolved(args: List<Any>, resources: Resources): Array<Any> =
    Array(args.size) { i -> (args[i] as? UiText)?.resolve(resources) ?: args[i] }
