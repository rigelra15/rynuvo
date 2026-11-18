package id.hypertopia.rynuvo

import android.Manifest
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.net.Uri
import android.provider.Settings
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.google.firebase.messaging.FirebaseMessaging
import java.io.File
import org.json.JSONObject

class MainActivity : TauriActivity() {
  // Back closes the open chat/room (web history); on the list it sends Rynuvo to the background
  // like any chat app, instead of finishing the activity, which ends the process and drops voice.
  override val handleBackNavigation: Boolean = false
  private var web: WebView? = null

  override fun onWebViewCreate(webView: WebView) {
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        // Something on the page that Back should close first (the QR scanner) says so here.
        webView.evaluateJavascript("!!(window.__rynuvoBack && window.__rynuvoBack())") { handled ->
          if (handled == "true") return@evaluateJavascript
          if (webView.canGoBack()) webView.goBack() else moveTaskToBack(true)
        }
      }
    })
    web = webView
    webView.addJavascriptInterface(Bridge(), "RynuvoAndroid")
    // Buttons on the call notification go to the page, which owns the call.
    CallService.onAction = { a -> webView.post { webView.evaluateJavascript("window.__rynuvoCall && window.__rynuvoCall('$a')", null) } }
    // A new Firebase token: the page signs this phone up with it.
    PushService.onToken = { t -> webView.post { webView.evaluateJavascript("window.__rynuvoPushToken && window.__rynuvoPushToken(${JSONObject.quote(t)})", null) } }
  }

  /** A chat or room a notification asked to open; the page takes it (now, or once it's up). */
  @Volatile private var pendingOpen: Long = 0

  private fun takeIntent(i: Intent?) {
    val ch = i?.getLongExtra(PushService.EXTRA_CHANNEL, 0) ?: 0
    if (ch == 0L) return
    i?.removeExtra(PushService.EXTRA_CHANNEL)
    pendingOpen = ch
    web?.post { web?.evaluateJavascript("window.__rynuvoOpen && window.__rynuvoOpen($ch)", null) }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    takeIntent(intent)
  }

  override fun onResume() {
    super.onResume()
    PushService.foreground = true
    PushService.clearAll(this)
  }

  override fun onPause() {
    PushService.foreground = false
    super.onPause()
  }

  override fun onDestroy() {
    // The page (and with it the call) is going away.
    if (isFinishing) CallService.hide(this)
    super.onDestroy()
  }

  /** Phones stay upright; a stream shown full screen may turn with the device. Tablets always turn. */
  private val isPhone get() = resources.configuration.smallestScreenWidthDp < 600

  private fun lockUpright(locked: Boolean) {
    if (!isPhone) return
    requestedOrientation = if (locked) ActivityInfo.SCREEN_ORIENTATION_PORTRAIT else ActivityInfo.SCREEN_ORIENTATION_FULL_USER
  }

  /** A stream full screen: the status and navigation bars get out of the way (a swipe brings them back). */
  private fun immersive(on: Boolean) {
    val c = WindowCompat.getInsetsController(window, window.decorView)
    if (on) {
      c.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
      c.hide(WindowInsetsCompat.Type.systemBars())
    } else c.show(WindowInsetsCompat.Type.systemBars())
  }

  inner class Bridge {
    @JavascriptInterface
    fun setFreeRotation(free: Boolean) = runOnUiThread {
      lockUpright(!free)
      immersive(free)
    }

    /** In a call: show (or update) the call notification. */
    @JavascriptInterface
    fun callShow(room: String, muted: Boolean, deafened: Boolean) = runOnUiThread {
      runCatching { CallService.show(this@MainActivity, room, muted, deafened) }
    }

    /** Out of the call: the notification goes, and the app may be put to sleep again. */
    @JavascriptInterface
    fun callHide() = runOnUiThread { CallService.hide(this@MainActivity) }

    /** The welcome screens: where one permission stands ("granted", "ask", or "blocked" when Android
     *  won't show its dialog again and only Settings can turn it on). `name` is "mic" or "notifications". */
    @JavascriptInterface
    fun permission(name: String): String = permissionState(name)

    /** Show Android's dialog for one permission; the answer goes to window.__rynuvoPerm(name, state). */
    @JavascriptInterface
    fun askPermission(name: String) = runOnUiThread {
      val p = manifestName(name)
      if (p == null || permissionState(name) == "granted") return@runOnUiThread answer(name)
      getSharedPreferences("perm", MODE_PRIVATE).edit().putBoolean(name, true).apply()
      ActivityCompat.requestPermissions(this@MainActivity, arrayOf(p), codeOf(name))
    }

    /** Rynuvo's page in Android's settings, for a permission that can only be turned on there. */
    @JavascriptInterface
    fun openAppSettings() = runOnUiThread {
      runCatching { startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName"))) }
    }

    /** After the splash: ask for what calls need (the mic) and their controls (notifications). */
    @JavascriptInterface
    fun requestPermissions() = runOnUiThread { requestRuntimePermissions() }

    /** Whether Android lets this app open the installer (the user's "install unknown apps" switch). */
    @JavascriptInterface
    fun canInstall(): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.O || packageManager.canRequestPackageInstalls()

    /** The settings page for that switch, for when it's off. */
    @JavascriptInterface
    fun allowInstall() = runOnUiThread {
      runCatching {
        startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
      }
    }

    /** This phone's Firebase token ("" until Firebase has one; it then comes to window.__rynuvoPushToken). */
    @JavascriptInterface
    fun pushToken(): String {
      val saved = PushService.prefs(this@MainActivity).getString("token", null)
      if (saved != null) return saved
      runCatching {
        FirebaseMessaging.getInstance().token.addOnSuccessListener { t ->
          PushService.prefs(this@MainActivity).edit().putString("token", t).apply()
          PushService.onToken?.invoke(t)
        }
      }
      return ""
    }

    /** Signed up: where notifications are fetched from, and the key for it. */
    @JavascriptInterface
    fun pushSetup(server: String, key: String) = PushService.setup(this@MainActivity, server, key)

    /** Signed out: no more notifications on this phone. */
    @JavascriptInterface
    fun pushClear() = PushService.clearSetup(this@MainActivity)

    /** The chat or room a tapped notification is for (0: none), once. */
    @JavascriptInterface
    fun takeOpen(): Long {
      val ch = pendingOpen
      pendingOpen = 0
      return ch
    }

    /** Open the system installer on a downloaded update (Android asks the user to confirm). */
    @JavascriptInterface
    fun installApk(path: String): Boolean {
      val file = File(path)
      if (!file.exists()) return false
      return runCatching {
        val uri = FileProvider.getUriForFile(this@MainActivity, "$packageName.fileprovider", file)
        startActivity(Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(uri, "application/vnd.android.package-archive")
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        })
        true
      }.getOrDefault(false)
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    lockUpright(true)
    takeIntent(intent)
    // Permissions are asked for once the splash has gone (the page calls requestPermissions), so the
    // system dialogs don't cover the opening.
  }

  private fun manifestName(name: String): String? = when (name) {
    "mic" -> Manifest.permission.RECORD_AUDIO
    "notifications" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) Manifest.permission.POST_NOTIFICATIONS else null
    else -> null
  }

  private fun codeOf(name: String) = if (name == "mic") 11 else 12

  private fun permissionState(name: String): String {
    if (name == "notifications" && !NotificationManagerCompat.from(this).areNotificationsEnabled()) {
      // Before Android 13 (or switched off by hand) there's no dialog: only Settings turns them on.
      if (manifestName(name) == null) return "blocked"
    }
    val p = manifestName(name) ?: return "granted"
    if (ContextCompat.checkSelfPermission(this, p) == PackageManager.PERMISSION_GRANTED) return "granted"
    // Asked before and Android no longer offers a rationale: the dialog won't come up again.
    val asked = getSharedPreferences("perm", MODE_PRIVATE).getBoolean(name, false)
    return if (asked && !ActivityCompat.shouldShowRequestPermissionRationale(this, p)) "blocked" else "ask"
  }

  private fun answer(name: String) {
    val state = permissionState(name)
    web?.post { web?.evaluateJavascript("window.__rynuvoPerm && window.__rynuvoPerm('$name','$state')", null) }
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    when (requestCode) {
      11 -> answer("mic")
      12 -> answer("notifications")
    }
  }

  // Voice reads the mic from native code (cpal/AAudio), which can't ask for it itself.
  private fun requestRuntimePermissions() {
    val wanted = mutableListOf(Manifest.permission.RECORD_AUDIO)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) wanted += Manifest.permission.POST_NOTIFICATIONS
    val missing = wanted.filter { ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED }
    if (missing.isNotEmpty()) ActivityCompat.requestPermissions(this, missing.toTypedArray(), 1)
  }
}
