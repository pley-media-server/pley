package app.pley.android.ui.phone.admin.users

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.maxLength
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.InputChip
import androidx.compose.material3.InputChipDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.admin.LabelUse
import app.pley.android.model.admin.RatingSystems
import app.pley.android.screens.admin.users.AGES
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.screens.admin.users.BIRTHDAY_LIMIT_ENDS
import app.pley.android.screens.admin.users.LimitMode
import app.pley.android.screens.admin.users.MAX_LABEL
import app.pley.android.screens.admin.users.MIN_PASSWORD_LENGTH
import app.pley.android.screens.admin.users.PasswordProblem
import app.pley.android.screens.admin.users.RatingFor
import app.pley.android.screens.admin.users.addLabel
import app.pley.android.screens.admin.users.ageLevels
import app.pley.android.screens.admin.users.birthMonthOf
import app.pley.android.screens.admin.users.birthYears
import app.pley.android.screens.admin.users.birthdayLimitEnd
import app.pley.android.screens.admin.users.inBoth
import app.pley.android.screens.admin.users.labelKey
import app.pley.android.screens.admin.users.limitMode
import app.pley.android.screens.admin.users.limitRating
import app.pley.android.screens.admin.users.parseBirthMonth
import app.pley.android.screens.admin.users.ratingSystem
import app.pley.android.screens.admin.users.sameLabels
import app.pley.android.screens.admin.users.toggled
import app.pley.android.screens.admin.users.withAge
import app.pley.android.screens.admin.users.withBirthMonth
import app.pley.android.screens.admin.users.withMode
import app.pley.android.screens.admin.users.withSystem
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.SegmentedControl
import java.text.Collator
import java.time.Month
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

/*
 * What an account sees, chosen for an account and for an invitation alike
 * (the web's AccessFields and lib/access.ts): every library or only some,
 * an age limit picked by rating, by age or by birthday, and its exceptions
 * by label. Also the small pieces Users and Invitations share: the tags of
 * a row, a switch row that can be off limits, and the words of a
 * password's problem.
 */

/**
 * What an age limit and its exceptions are picked from, each `null` while
 * unknown (or from a server before it): the rating systems, the labels
 * titles carry, and whether a TMDB key gives movies their ratings.
 */
internal data class AccessSources(
    val systems: RatingSystems? = null,
    val labels: List<LabelUse>? = null,
    val tmdbKey: Boolean? = null,
)

/**
 * An account's (or an invitation's) access: Libraries, which opens
 * [LibrariesSheet]; Age limit, how it's set (no limit, by rating, by age,
 * or by [birthday] for an account), then that way's rows, with what the
 * limit shows under them; and the exceptions by label. Each choice is
 * passed on whole, the limit kept as near as it can be ([withMode]), as
 * the web's AccessFields does. [libraries]: the server's, `null` while
 * unknown. [pickLibrary]: what the sheet says when "Only some" has none
 * chosen. Without [limits] (an administrator, whom no limit or exception
 * holds back, and whose answer the server sends without them), Libraries
 * alone.
 */
@Composable
internal fun AccessGroup(
    access: AccessChoice,
    libraries: List<Library>?,
    sources: AccessSources,
    onLibraries: (all: Boolean, ids: List<Long>) -> Unit,
    onAgeLimit: (AccessChoice) -> Unit,
    onLabels: (show: List<String>, hide: List<String>) -> Unit,
    pickLibrary: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    birthday: Boolean = false,
    limits: Boolean = true,
) {
    var sheet by remember { mutableStateOf<AccessSheet?>(null) }
    val systems = sources.systems
    val mode = limitMode(systems, access)
    val ratingFor = ratingForWords()
    val country = ratingSystem(systems, access.maxAgeSystem)
    val locale = shownLocale()
    val born = access.birthMonth?.let(::parseBirthMonth)
    val change = { next: AccessChoice -> if (next != access) onAgeLimit(next) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(28.dp)) {
        SettingsGroup(null, footer = if (limits) limitHint(access, systems, ratingFor) else null) {
            SettingsRow(
                stringResource(R.string.admin_users_access_libraries),
                value = librariesLabel(access.allLibraries, access.libraryIds, libraries),
                icon = PleyIcons.LibraryBig,
                enabled = enabled,
                onClick = { sheet = AccessSheet.Libraries },
            )
            if (!limits) return@SettingsGroup
            RowDivider()
            SettingsRow(
                stringResource(R.string.admin_users_access_age_limit),
                value = modeLabel(mode),
                icon = PleyIcons.Lock,
                enabled = enabled,
                onClick = { sheet = AccessSheet.Mode },
            )
            if (mode == LimitMode.Rating && country != null) {
                val level = ageLevels(systems, country, ratingFor).firstOrNull { it.age == access.maxAge }
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_limit_rating_system),
                    value = countryName(country, systems, locale),
                    icon = PleyIcons.Globe,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.System },
                )
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_limit_rating),
                    value = level?.rating?.let { stringResource(R.string.admin_users_limit_rating_option, it, level.age) },
                    icon = PleyIcons.Star,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.Rating },
                )
            }
            if (mode == LimitMode.Age) {
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_limit_age),
                    value = access.maxAge?.toString(),
                    icon = PleyIcons.UserRound,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.Age },
                )
            }
            if (mode == LimitMode.Birthday && born != null) {
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_limit_birth_month),
                    value = monthName(born.monthValue, locale),
                    icon = PleyIcons.CalendarDays,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.Month },
                )
                RowDivider()
                SettingsRow(
                    stringResource(R.string.admin_users_limit_birth_year),
                    value = born.year.toString(),
                    icon = PleyIcons.CalendarDays,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.Year },
                )
            }
        }
        if (!limits) return@Column
        if (access.maxAge != null && sources.tmdbKey == false) UnratedWarning()
        SettingsGroup(stringResource(R.string.admin_users_exceptions), footer = stringResource(R.string.admin_users_exceptions_hint)) {
            // Only an age limit hides what these would show; kept as they are while there's none.
            if (mode != LimitMode.None) {
                SettingsRow(
                    stringResource(R.string.admin_users_show_labels),
                    value = labelsValue(access.showLabels),
                    icon = PleyIcons.Eye,
                    enabled = enabled,
                    onClick = { sheet = AccessSheet.Show },
                )
                RowDivider()
            }
            SettingsRow(
                stringResource(R.string.admin_users_hide_labels),
                value = labelsValue(access.hideLabels),
                icon = PleyIcons.EyeOff,
                enabled = enabled,
                onClick = { sheet = AccessSheet.Hide },
            )
        }
    }
    val dismiss = { sheet = null }
    when (sheet) {
        AccessSheet.Libraries -> LibrariesSheet(
            all = access.allLibraries,
            ids = access.libraryIds,
            libraries = libraries.orEmpty(),
            pickLibrary = pickLibrary,
            onSave = { all, ids ->
                sheet = null
                onLibraries(all, ids)
            },
            onDismiss = dismiss,
        )
        AccessSheet.Mode -> ChoiceSheet(
            title = stringResource(R.string.admin_users_access_age_limit),
            choices = listOfNotNull(
                LimitMode.None,
                LimitMode.Rating.takeIf { country != null },
                LimitMode.Age,
                LimitMode.Birthday.takeIf { birthday },
            ).map { Choice(it, modeLabel(it)) },
            selected = mode,
            onPick = { if (it != mode) change(access.withMode(it, systems, birthday)) },
            onDismiss = dismiss,
        )
        AccessSheet.System -> ChoiceSheet(
            title = stringResource(R.string.admin_users_limit_rating_system),
            choices = systems?.systems.orEmpty()
                .map { Choice<String?>(it.country, countryName(it.country, systems, locale)) }
                .sortedWith(compareBy(Collator.getInstance(locale)) { it.label }),
            selected = country,
            onPick = { system -> if (system != null) change(access.withSystem(systems, system)) },
            onDismiss = dismiss,
        )
        AccessSheet.Rating -> ChoiceSheet(
            title = stringResource(R.string.admin_users_limit_rating),
            choices = country?.let { ageLevels(systems, it, ratingFor) }.orEmpty()
                .map { Choice<Int?>(it.age, stringResource(R.string.admin_users_limit_rating_option, it.rating.orEmpty(), it.age)) },
            selected = access.maxAge,
            onPick = { age -> change(access.copy(maxAge = age)) },
            onDismiss = dismiss,
        )
        AccessSheet.Age -> ChoiceSheet(
            title = stringResource(R.string.admin_users_limit_age),
            choices = AGES.map { Choice<Int?>(it, it.toString()) },
            selected = access.maxAge,
            onPick = { age -> if (age != null) change(access.withAge(age)) },
            onDismiss = dismiss,
        )
        AccessSheet.Month -> if (born != null) {
            ChoiceSheet(
                title = stringResource(R.string.admin_users_limit_birth_month),
                choices = (1..12).map { Choice(it, monthName(it, locale)) },
                selected = born.monthValue,
                onPick = { month -> change(access.withBirthMonth(birthMonthOf(born.year, month))) },
                onDismiss = dismiss,
            )
        }
        AccessSheet.Year -> if (born != null) {
            ChoiceSheet(
                title = stringResource(R.string.admin_users_limit_birth_year),
                choices = birthYears(access.birthMonth).map { Choice(it, it.toString()) },
                selected = born.year,
                onPick = { year -> change(access.withBirthMonth(birthMonthOf(year, born.monthValue))) },
                onDismiss = dismiss,
            )
        }
        AccessSheet.Show -> LabelsSheet(
            title = stringResource(R.string.admin_users_show_labels),
            hint = stringResource(R.string.admin_users_show_labels_hint),
            initial = access.showLabels,
            other = access.hideLabels,
            known = sources.labels.orEmpty(),
            onSave = { show ->
                sheet = null
                if (!sameLabels(show, access.showLabels)) onLabels(show, access.hideLabels)
            },
            onDismiss = dismiss,
        )
        AccessSheet.Hide -> LabelsSheet(
            title = stringResource(R.string.admin_users_hide_labels),
            hint = stringResource(R.string.admin_users_hide_labels_hint),
            initial = access.hideLabels,
            other = access.showLabels,
            known = sources.labels.orEmpty(),
            onSave = { hide ->
                sheet = null
                if (!sameLabels(hide, access.hideLabels)) onLabels(access.showLabels, hide)
            },
            onDismiss = dismiss,
        )
        null -> Unit
    }
}

private enum class AccessSheet { Libraries, Mode, System, Rating, Age, Month, Year, Show, Hide }

@Composable
private fun modeLabel(mode: LimitMode): String = stringResource(
    when (mode) {
        LimitMode.None -> R.string.admin_users_limit_none
        LimitMode.Rating -> R.string.admin_users_limit_by_rating
        LimitMode.Age -> R.string.admin_users_limit_by_age
        LimitMode.Birthday -> R.string.admin_users_limit_by_birthday
    },
)

/**
 * What the limit shows, under its rows (the web's hint): every title,
 * those suitable for its age and which ratings that is, and when one that
 * follows a birthday goes up and ends.
 */
@Composable
private fun limitHint(access: AccessChoice, systems: RatingSystems?, ratingFor: RatingFor): String {
    val maxAge = access.maxAge
    val birthMonth = access.birthMonth
    val limit = when {
        maxAge == null && birthMonth != null -> stringResource(R.string.admin_users_limit_hint_grown_up, BIRTHDAY_LIMIT_ENDS)
        maxAge == null -> stringResource(R.string.admin_users_limit_hint_none)
        else -> {
            val rating = limitRating(systems, access.maxAgeSystem, maxAge, ratingFor)
            when {
                rating != null && maxAge == 0 -> stringResource(R.string.admin_users_limit_hint_rating_all_ages, rating)
                rating != null -> stringResource(R.string.admin_users_limit_hint_rating, maxAge, rating)
                maxAge == 0 -> stringResource(R.string.admin_users_limit_hint_age_all_ages)
                else -> stringResource(R.string.admin_users_limit_hint_age, maxAge)
            }
        }
    }
    val born = birthMonth?.let(::parseBirthMonth)
    val end = birthMonth?.let(::birthdayLimitEnd)
    if (maxAge == null || born == null || end == null) return limit
    val locale = shownLocale()
    val ends = DateTimeFormatter.ofPattern("LLLL yyyy", locale).format(end)
    return limit + " " + stringResource(R.string.admin_users_limit_hint_birthday, monthName(born.monthValue, locale), ends)
}

/** A month's name in [locale], as the web's month list says it ("March", "mars"). */
private fun monthName(month: Int, locale: Locale): String = Month.of(month).getDisplayName(TextStyle.FULL_STANDALONE, locale)

/** A country's name in [locale] ("United Kingdom", "Royaume-Uni"), else the server's. */
private fun countryName(code: String, systems: RatingSystems?, locale: Locale): String {
    val name = runCatching { Locale.Builder().setRegion(code).build().getDisplayCountry(locale) }.getOrNull()
    return name?.takeIf { it.isNotEmpty() && it != code } ?: systems?.systems?.firstOrNull { it.country == code }?.name ?: code
}

/** How a rating's name says film or TV when it's at two ages: "PG (film)". */
@Composable
internal fun ratingForWords(): RatingFor {
    val resources = LocalResources.current
    return { name, kind -> resources.getString(if (kind == "tv") R.string.admin_users_rating_for_tv else R.string.admin_users_rating_for_film, name) }
}

/** A list of labels as a row shows it: "Kids, Cartoons", or "None". */
@Composable
private fun labelsValue(labels: List<String>): String =
    labels.joinToString(", ").ifEmpty { stringResource(R.string.admin_users_exceptions_none) }

/**
 * An age limit hides unrated titles, and without a TMDB key movies get a
 * rating only from their NFO files: says so, as the web does. The key is
 * added in the web app, which holds the server's keys.
 */
@Composable
private fun UnratedWarning() {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(PleyShape.Card)
            .background(PleyColors.Amber.copy(alpha = 0.08f))
            .padding(16.dp),
    ) {
        Icon(PleyIcons.TriangleAlert, contentDescription = null, size = 18.dp, tint = PleyColors.Amber)
        Spacer(Modifier.width(12.dp))
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(stringResource(R.string.admin_users_limit_unrated_title), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg)
            Text(stringResource(R.string.admin_users_limit_unrated_text), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

/**
 * Choosing labels (the web's LabelPicker), saved together: those chosen
 * as chips, each removed by a tap, then a field whose text the list under
 * it narrows: the labels titles carry, with how many, and the text itself
 * as a new label. Done on the keyboard adds what's typed. A label also in
 * [other], the opposite list, is refused before anything is sent, as the
 * web refuses it.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LabelsSheet(
    title: String,
    hint: String,
    initial: List<String>,
    other: List<String>,
    known: List<LabelUse>,
    onSave: (List<String>) -> Unit,
    onDismiss: () -> Unit,
) {
    var chosen by remember { mutableStateOf(initial) }
    var clash by remember { mutableStateOf(emptyList<String>()) }
    val text = rememberTextFieldState()
    val query = text.text.toString().trim()
    val chosenKeys = chosen.map(::labelKey).toSet()
    val matches = known
        .filter { labelKey(it.label) !in chosenKeys && it.label.contains(query, ignoreCase = true) }
        // The label typed exactly, then those it starts, then the rest.
        .sortedBy { suggestionRank(it.label, query) }
        .take(MAX_SUGGESTIONS)
    val isNew = query.isNotEmpty() && labelKey(query) !in chosenKeys && known.none { labelKey(it.label) == labelKey(query) }
    val add = { label: String ->
        chosen = addLabel(chosen, label)
        text.clearText()
    }
    PleySheet(onDismiss = onDismiss, title = title) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(hint, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            if (chosen.isNotEmpty()) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    chosen.forEach { label ->
                        val remove = stringResource(R.string.admin_users_remove_label, label)
                        InputChip(
                            selected = false,
                            onClick = { chosen = chosen - label },
                            label = { Text(label, style = MaterialTheme.typography.labelLarge) },
                            trailingIcon = { Icon(PleyIcons.X, contentDescription = null, size = 16.dp, tint = PleyColors.Muted) },
                            modifier = Modifier.semantics { contentDescription = remove },
                            shape = PleyShape.Control,
                            colors = InputChipDefaults.inputChipColors(containerColor = PleyColors.Surface3, labelColor = PleyColors.Fg),
                            border = null,
                        )
                    }
                }
            }
            OutlinedTextField(
                state = text,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text(stringResource(R.string.admin_users_label_placeholder)) },
                lineLimits = TextFieldLineLimits.SingleLine,
                inputTransformation = InputTransformation.maxLength(MAX_LABEL),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                onKeyboardAction = { if (query.isNotEmpty()) add(query) },
                shape = PleyShape.Field,
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = PleyColors.AccentBright,
                    unfocusedBorderColor = PleyColors.Control,
                    cursorColor = PleyColors.AccentBright,
                ),
            )
        }
        if (matches.isNotEmpty() || isNew) {
            val suggestions = stringResource(R.string.admin_users_label_suggestions)
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 280.dp).padding(top = 4.dp).semantics { contentDescription = suggestions }) {
                items(matches, key = { "known:" + it.label }) { use ->
                    SuggestionRow(
                        use.label,
                        if (use.items > 0) pluralStringResource(R.plurals.admin_users_label_titles, use.items, use.items) else stringResource(R.string.admin_users_label_in_rules),
                    ) { add(use.label) }
                }
                if (isNew) {
                    item(key = "new") {
                        SuggestionRow(stringResource(R.string.admin_users_label_create, query), null, icon = PleyIcons.Plus) { add(query) }
                    }
                }
            }
        }
        if (clash.isNotEmpty()) {
            Text(
                stringResource(R.string.admin_users_labels_in_both, clash.joinToString(", ")),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.DangerText,
                modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 12.dp),
            )
        }
        PrimaryButton(
            stringResource(R.string.admin_save),
            onClick = {
                // What's typed and not yet added counts, as it would be lost.
                val labels = if (query.isNotEmpty()) addLabel(chosen, query) else chosen
                clash = inBoth(labels, other)
                if (clash.isEmpty()) onSave(labels)
            },
            modifier = Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 16.dp),
        )
    }
}

/** Most suggestions listed at once, as the web's; typing narrows the rest. */
private const val MAX_SUGGESTIONS = 50

/** The label typed exactly first, then those it starts, then the rest. */
private fun suggestionRank(label: String, query: String): Int = when {
    label.equals(query, ignoreCase = true) -> 0
    label.startsWith(query, ignoreCase = true) -> 1
    else -> 2
}

@Composable
private fun SuggestionRow(label: String, detail: String?, icon: PleyIcon? = null, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 18.dp, tint = PleyColors.AccentBright)
            Spacer(Modifier.width(12.dp))
        }
        Text(label, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        if (detail != null) {
            Spacer(Modifier.width(12.dp))
            Text(detail, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
        }
    }
}

/**
 * Choosing the libraries: all of them (those added later too), or only
 * the ones checked, saved together. Save with none checked says
 * [pickLibrary] instead, as the web does.
 */
@Composable
internal fun LibrariesSheet(
    all: Boolean,
    ids: List<Long>,
    libraries: List<Library>,
    pickLibrary: String,
    onSave: (all: Boolean, ids: List<Long>) -> Unit,
    onDismiss: () -> Unit,
) {
    var allNow by remember { mutableStateOf(all) }
    var chosen by remember { mutableStateOf(ids) }
    var tried by remember { mutableStateOf(false) }
    val none = !allNow && chosen.isEmpty()
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.admin_users_access_libraries)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            SegmentedControl(
                options = listOf(stringResource(R.string.admin_users_access_all_libraries), stringResource(R.string.admin_users_access_only_some)),
                selected = if (allNow) 0 else 1,
                onSelect = { allNow = it == 0 },
                modifier = Modifier.fillMaxWidth(),
            )
            if (allNow) {
                Text(stringResource(R.string.admin_users_access_includes_later), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            } else if (libraries.isEmpty()) {
                Text(stringResource(R.string.admin_users_access_no_libraries_yet), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
        }
        if (!allNow && libraries.isNotEmpty()) {
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp).padding(top = 8.dp)) {
                items(libraries, key = { it.id }) { library ->
                    val on = library.id in chosen
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 52.dp)
                            .toggleable(value = on, role = Role.Checkbox) { chosen = toggled(chosen, library.id) }
                            .padding(horizontal = 24.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = on,
                            onCheckedChange = null,
                            colors = CheckboxDefaults.colors(
                                checkedColor = PleyColors.Accent,
                                uncheckedColor = PleyColors.Control,
                                checkmarkColor = PleyColors.OnAccent,
                            ),
                        )
                        Spacer(Modifier.width(16.dp))
                        Icon(kindIcon(library.kind), contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
                        Spacer(Modifier.width(12.dp))
                        Text(library.name, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
        Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 16.dp)) {
            if (tried && none) {
                Text(pickLibrary, style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, modifier = Modifier.padding(bottom = 12.dp))
            }
            PrimaryButton(
                stringResource(R.string.admin_save),
                onClick = {
                    tried = true
                    if (!none) onSave(allNow, if (allNow) ids else chosen)
                },
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** A library's icon, as the web's AccessFields shows it. */
private fun kindIcon(kind: LibraryKind): PleyIcon = when (kind) {
    LibraryKind.Movies -> PleyIcons.Clapperboard
    LibraryKind.Shows -> PleyIcons.Tv
    LibraryKind.Music -> PleyIcons.Music
    LibraryKind.Unknown -> PleyIcons.LibraryBig
}

/** "All libraries", "Movies", "2 libraries", "No libraries". */
@Composable
internal fun librariesLabel(all: Boolean, ids: List<Long>, libraries: List<Library>?): String {
    val only = ids.singleOrNull()?.let { id -> libraries?.firstOrNull { it.id == id } }
    return when {
        all -> stringResource(R.string.admin_users_access_all_libraries)
        ids.isEmpty() -> stringResource(R.string.admin_users_access_no_libraries)
        only != null -> only.name
        else -> pluralStringResource(R.plurals.admin_users_access_libraries_count, ids.size, ids.size)
    }
}

/**
 * A row's age limit tag, as the web's badge says it: "Up to PG-13", "Up to
 * age 8", "Up to PG-13 (age 13, by birthday)"; `null` without a limit.
 */
@Composable
internal fun ageTag(systems: RatingSystems?, system: String?, maxAge: Int?, birthMonth: String? = null): String? {
    if (maxAge == null) return null
    val rating = limitRating(systems, system, maxAge, ratingForWords())
    return when {
        birthMonth != null && rating != null -> stringResource(R.string.admin_users_up_to_by_birthday, rating, maxAge)
        birthMonth != null -> stringResource(R.string.admin_users_up_to_age_by_birthday, maxAge)
        rating != null -> stringResource(R.string.admin_users_up_to, rating)
        else -> stringResource(R.string.admin_users_up_to_age, maxAge)
    }
}

/** The words of [problem], for a password's dialog. */
@Composable
internal fun passwordWords(): (PasswordProblem) -> String {
    val resources = LocalResources.current
    return { problem ->
        when (problem) {
            PasswordProblem.TooShort -> resources.getQuantityString(R.plurals.admin_users_password_too_short, MIN_PASSWORD_LENGTH, MIN_PASSWORD_LENGTH)
            PasswordProblem.SameAsUsername -> resources.getString(R.string.admin_users_password_same_as_username)
        }
    }
}

/** How a [Tag] reads: as the web's badges' tones. */
internal enum class TagTone { Neutral, Accent, Info, Success, Danger }

/** A small tag on a row ("Admin", "PIN", "Active"), in [tone]. */
@Composable
internal fun Tag(text: String, tone: TagTone = TagTone.Neutral, icon: PleyIcon? = null) {
    val (fill, ink) = when (tone) {
        TagTone.Neutral -> PleyColors.Surface3 to PleyColors.Muted
        TagTone.Accent -> PleyColors.PrimaryContainer to PleyColors.AccentBright
        TagTone.Info -> PleyColors.Info.copy(alpha = 0.16f) to PleyColors.Info
        TagTone.Success -> PleyColors.Success.copy(alpha = 0.16f) to PleyColors.Success
        TagTone.Danger -> PleyColors.Danger.copy(alpha = 0.16f) to PleyColors.DangerText
    }
    Row(
        Modifier.background(fill, PleyShape.Badge).padding(horizontal = 6.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 12.dp, tint = ink)
            Spacer(Modifier.width(4.dp))
        }
        Text(text, style = MaterialTheme.typography.labelSmall, color = ink, maxLines = 1)
    }
}

/** Text that isn't a row: a group's "Loading…" or "No apps use this account.". */
@Composable
internal fun GroupLine(text: String, color: Color = PleyColors.Muted) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = color, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 16.dp))
}
