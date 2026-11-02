package app.pley.android.ui.tv.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyType
import app.pley.android.screens.onboarding.ServersUiState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.kit.digitKeys
import kotlinx.coroutines.flow.drop

/**
 * Typing a server's address (design spec §2.11; tv-addendum §4.4): a field
 * and Connect under it. The TV's keyboard opens with the screen, so a name
 * or a whole https address types as easily as an IP; once it is closed,
 * the remote's digit keys type too. Port 7539 is used by default.
 */
@Composable
internal fun TvAddressEntry(
    state: ServersUiState,
    onConnect: (String) -> Unit,
    onEdited: () -> Unit,
) {
    val field = rememberTextFieldState()
    LaunchedEffect(field) {
        snapshotFlow { field.text.toString() }.drop(1).collect { onEdited() }
    }
    val connect = { if (field.text.isNotBlank()) onConnect(field.text.toString().trim()) }
    Box(
        Modifier
            .fillMaxSize()
            .brandGlow()
            .digitKeys(
                onDigit = { d -> field.type(d.toString()) },
                onDelete = { field.deleteLast() },
                onDot = { field.type(".") },
                onText = { c -> field.type(c.toString()) },
            )
            .testTag(ADDRESS_ENTRY_TAG),
    ) {
        Column(
            Modifier.fillMaxWidth().padding(top = 27.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            BasicText(stringResource(R.string.tv_onboarding_address_title), style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
            Spacer(Modifier.height(8.dp))
            TvTextField(
                state = field,
                placeholder = stringResource(R.string.tv_onboarding_address_placeholder),
                modifier = Modifier.width(FieldWidth).testTag(ADDRESS_FIELD_TAG),
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Go,
                onAction = connect,
                typeNow = true,
            )
            Spacer(Modifier.height(14.dp))
            TvPillButton(
                label = stringResource(if (state.checkingAddress) R.string.tv_onboarding_checking else R.string.tv_onboarding_connect),
                onClick = connect,
                modifier = Modifier.testTag(CONNECT_TAG),
            )
            Spacer(Modifier.height(24.dp))
            // Under it: how addresses work, and what went wrong.
            Column(
                Modifier.width(FieldWidth).semantics { liveRegion = LiveRegionMode.Polite },
                verticalArrangement = Arrangement.spacedBy(12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                BasicText(
                    stringResource(R.string.tv_onboarding_address_hint),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
                )
                state.addressProblem?.let {
                    BasicText(
                        problemText(it),
                        style = PleyType.Tv.Body.copy(color = PleyColors.DangerText, textAlign = TextAlign.Center),
                        modifier = Modifier.testTag(ADDRESS_PROBLEM_TAG),
                    )
                }
            }
        }
    }
}

private fun TextFieldState.type(text: String) = edit { append(text) }

private fun TextFieldState.deleteLast() = edit { if (length > 0) replace(length - 1, length, "") }

private val FieldWidth = 480.dp

internal const val ADDRESS_ENTRY_TAG = "pley.servers.address"
internal const val ADDRESS_FIELD_TAG = "pley.servers.address.field"
internal const val ADDRESS_PROBLEM_TAG = "pley.servers.address.problem"
internal const val CONNECT_TAG = "pley.servers.address.connect"
