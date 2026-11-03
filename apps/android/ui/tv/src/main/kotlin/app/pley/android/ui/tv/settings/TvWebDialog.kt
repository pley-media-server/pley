package app.pley.android.ui.tv.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Surface
import androidx.tv.material3.SurfaceDefaults
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.OverlayShowing
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.onboarding.QrCodeImage

/**
 * A page of pley's web app, for a TV, which opens no browser (TV-WB): the
 * address to type on a phone or computer, and its QR code to scan
 * (design spec §4.8, "Manage your account on the web"). Back or Close
 * dismisses it; focus starts on Close.
 */
@Composable
internal fun TvWebDialog(server: String, path: String, onDismiss: () -> Unit) {
    val url = server.trimEnd('/') + path
    val close = remember { FocusRequester() }
    BackHandler(onBack = onDismiss)
    OverlayShowing()
    LaunchedEffect(Unit) { runCatching { close.requestFocus() } }
    Box(Modifier.fillMaxSize().background(PleyColors.Scrim).testTag(TV_WEB_DIALOG), contentAlignment = Alignment.Center) {
        Surface(
            modifier = Modifier.widthIn(max = 640.dp).focusGroup(),
            shape = PleyShape.Dialog,
            colors = SurfaceDefaults.colors(containerColor = PleyColors.Surface2, contentColor = PleyColors.Fg),
        ) {
            Row(Modifier.padding(28.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.width(300.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    BasicText(stringResource(R.string.tv_account_web_title), style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
                    BasicText(stringResource(R.string.tv_account_web_body), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
                    BasicText(hostPort(url), style = PleyType.Tv.Title.copy(color = PleyColors.AccentBright))
                    Spacer(Modifier.padding(top = 8.dp))
                    TvPillButton(stringResource(R.string.tv_account_close), onClick = onDismiss, modifier = Modifier.focusRequester(close))
                }
                Spacer(Modifier.width(28.dp))
                Box(Modifier.clip(PleyShape.Dialog).background(PleyColors.QrLight).padding(4.dp)) {
                    QrCodeImage(url, 220.dp, Modifier.clearAndSetSemantics {})
                }
            }
        }
    }
}

internal const val TV_WEB_DIALOG = "pley.tv.settings.web"
