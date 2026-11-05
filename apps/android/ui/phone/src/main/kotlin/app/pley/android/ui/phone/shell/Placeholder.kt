package app.pley.android.ui.phone.shell

import android.content.pm.ApplicationInfo
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.icon.PleyMark
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.KitGallery
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton

/**
 * A screen that isn't built yet: its name in the top bar and a short
 * note, in the shell's frame, so the navigation can be tried end to end.
 * Debug builds can open the phone kit's gallery from it.
 */
@Composable
internal fun Placeholder(key: NavKey) {
    val navigator = LocalPhoneNavigator.current
    var kit by rememberSaveable { mutableStateOf(false) }
    if (kit) {
        BackHandler { kit = false }
        KitGallery(onBack = { kit = false })
        return
    }
    val name = (key as? PleyKey)?.let { screenName(it) } ?: key.javaClass.simpleName
    val debuggable = LocalContext.current.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
    Column(Modifier.fillMaxSize().testTag(TestTags.PLACEHOLDER)) {
        if (key == PleyKey.Home) {
            PleyTopAppBar(title = { Brand() })
        } else {
            val root = PhoneTab.rootOf(key as? PleyKey ?: PleyKey.Home) != null || (key as? PleyKey)?.isGettingIn == true
            PleyTopAppBar(title = name, onBack = if (root) null else navigator::back)
        }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            EmptyState(
                title = stringResource(R.string.shell_placeholder_title, name),
                body = stringResource(R.string.shell_placeholder_body),
                icon = PleyIcons.Sparkles,
                action = {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        when (key) {
                            // Debug builds only: past the screens of getting in, until they are built.
                            is PleyKey.Servers, is PleyKey.SignIn -> if (debuggable) {
                                TonalButton(stringResource(R.string.shell_placeholder_in), onClick = { navigator.restart(PleyKey.Home) })
                            }
                            PleyKey.Home, PleyKey.Libraries -> TonalButton(
                                stringResource(R.string.shell_placeholder_next),
                                onClick = { navigator.open(PleyKey.Item(PLACEHOLDER_ITEM)) },
                            )
                            else -> Unit
                        }
                        if (debuggable) {
                            PleyTextButton(stringResource(R.string.shell_placeholder_kit), onClick = { kit = true })
                        }
                    }
                },
            )
        }
    }
}

/** The pley mark and wordmark, as Home's top bar shows them. */
@Composable
internal fun Brand() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Image(rememberVectorPainter(PleyMark.Tile), contentDescription = null, modifier = Modifier.size(28.dp))
        Spacer(Modifier.width(10.dp))
        Text(stringResource(R.string.shell_brand), style = MaterialTheme.typography.headlineSmall, color = PleyColors.Fg)
    }
}

/** What the top bar calls the screen of [key]: a few by name, the rest by their route's. */
@Composable
private fun screenName(key: PleyKey): String = when (key) {
    is PleyKey.Search -> stringResource(R.string.shell_tab_search)
    is PleyKey.SignIn -> stringResource(R.string.shell_scope_action)
    is PleyKey.Item -> stringResource(R.string.shell_placeholder_item, key.itemId)
    else -> key.javaClass.simpleName
}

/** The id the placeholders open, to show a screen above a tab's first. */
private const val PLACEHOLDER_ITEM = 101L
