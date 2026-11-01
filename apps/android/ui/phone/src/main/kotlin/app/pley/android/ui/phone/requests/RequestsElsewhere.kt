package app.pley.android.ui.phone.requests

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.requests.ItemRequestState
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/*
 * What requests show on the app's other screens: the
 * ways in on Me, the number on Me's tab, and a library title's request.
 */

/**
 * Requests' rows on Me: Discover, and Your requests (Requests for
 * managers, with what it's for) carrying its number, red while failed
 * requests wait for a manager.
 */
@Composable
internal fun RequestsRows(entry: RequestsEntry) {
    val navigator = LocalPhoneNavigator.current
    val texts = rememberRequestTexts()
    if (entry.discover) {
        SettingsRow(
            stringResource(R.string.requests_discover),
            value = stringResource(R.string.requests_discover_hint),
            icon = PleyIcons.Compass,
            onClick = { navigator.open(PleyKey.Discover()) },
            modifier = Modifier.testTag(ME_DISCOVER),
        )
    }
    if (entry.discover && entry.requests) RowDivider()
    if (entry.requests) {
        SettingsRow(
            stringResource(if (entry.manage) R.string.requests_manage else R.string.requests_yours),
            value = if (entry.manage) stringResource(R.string.requests_manage_hint) else null,
            icon = PleyIcons.ListVideo,
            onClick = { navigator.open(PleyKey.Requests) },
            modifier = Modifier.testTag(ME_REQUESTS),
            trailing = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (entry.badge > 0) {
                        CountBadge(entry.badge, entry.alarm, texts.count(entry))
                        Spacer(Modifier.width(8.dp))
                    }
                    Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
                }
            },
        )
    }
}

/**
 * [icon] of Me's tab, with the number on the way into requests while there
 * is one, which screen readers say in words.
 */
@Composable
internal fun RequestsTabIcon(entry: RequestsEntry, icon: @Composable () -> Unit) {
    if (entry.badge <= 0) return icon()
    val description = rememberRequestTexts().count(entry)
    BadgedBox(
        badge = {
            Badge(
                modifier = Modifier.clearAndSetSemantics { contentDescription = description },
                containerColor = if (entry.alarm) PleyColors.Danger else PleyColors.Accent,
                contentColor = if (entry.alarm) PleyColors.Bg else PleyColors.OnAccent,
            ) {
                Text(if (entry.badge > MAX_TAB_COUNT) "$MAX_TAB_COUNT+" else entry.badge.toString(), style = MaterialTheme.typography.labelSmall)
            }
        },
    ) { icon() }
}

/**
 * What requests say of a library's movie or show, under its actions: the
 * stage of its request (with the download's bar), and Request more
 * seasons, which opens the title's Discover page with its picker.
 */
@Composable
internal fun ItemRequestBlock(state: ItemRequestState) {
    val request = state.request
    if (request == null && !state.more) return
    val navigator = LocalPhoneNavigator.current
    val texts = rememberRequestTexts()
    Column(Modifier.fillMaxWidth().padding(top = 12.dp).testTag(ITEM_REQUEST), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (request != null) RequestStage(request, state.manage, texts)
        if (state.more) {
            TonalButton(stringResource(R.string.requests_more_seasons), onClick = { navigator.open(state.picker) }, icon = PleyIcons.Plus)
        }
    }
}

/** What Me's tab counts to before it says "99+". */
private const val MAX_TAB_COUNT = 99

internal const val ME_DISCOVER = "pley.me.discover"
internal const val ME_REQUESTS = "pley.me.requests"
internal const val ITEM_REQUEST = "pley.item.request"
