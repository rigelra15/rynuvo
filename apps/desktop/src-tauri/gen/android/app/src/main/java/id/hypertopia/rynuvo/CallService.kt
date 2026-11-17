package id.hypertopia.rynuvo

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Keeps a call going with the app in the background: a foreground service (so Android keeps the
 * process and lets it use the mic) with a notification that stays (swiped away, it comes back), carrying
 * Mute, Deafen and End call. The page drives it through [MainActivity]'s JS bridge; the buttons
 * go back to the page through [onAction].
 */
class CallService : Service() {
  companion object {
    private const val CHANNEL = "call"
    private const val ID = 1
    private const val EXTRA_ROOM = "room"
    private const val EXTRA_MUTED = "muted"
    private const val EXTRA_DEAFENED = "deafened"
    private const val SHOW = "show"
    private const val DISMISSED = "dismissed"

    /** A button was pressed: "mute", "deafen" or "leave". Set by the activity. */
    @Volatile var onAction: ((String) -> Unit)? = null

    fun show(ctx: Context, room: String, muted: Boolean, deafened: Boolean) {
      val i = Intent(ctx, CallService::class.java).setAction(SHOW)
        .putExtra(EXTRA_ROOM, room).putExtra(EXTRA_MUTED, muted).putExtra(EXTRA_DEAFENED, deafened)
      ContextCompat.startForegroundService(ctx, i)
    }

    fun hide(ctx: Context) {
      ctx.stopService(Intent(ctx, CallService::class.java))
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    when (val a = intent?.action) {
      SHOW -> post(
        intent.getStringExtra(EXTRA_ROOM) ?: "Call",
        intent.getBooleanExtra(EXTRA_MUTED, false),
        intent.getBooleanExtra(EXTRA_DEAFENED, false),
      )
      "mute", "deafen", "leave" -> onAction?.invoke(a)
      // Swiped away (Android 14+ allows that even for a service's notification): put it back, the
      // call is still on. It goes only when the call ends.
      DISMISSED -> last?.let { (room, muted, deafened) -> post(room, muted, deafened) }
    }
    // Killed with the app: the call is gone too, so don't come back on our own.
    return START_NOT_STICKY
  }

  private var last: Triple<String, Boolean, Boolean>? = null

  private fun post(room: String, muted: Boolean, deafened: Boolean) {
    last = Triple(room, muted, deafened)
    val nm = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && nm.getNotificationChannel(CHANNEL) == null) {
      nm.createNotificationChannel(NotificationChannel(CHANNEL, "Calls", NotificationManager.IMPORTANCE_LOW).apply {
        description = "Shown while you're in a room"
        setShowBadge(false)
      })
    }
    val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    fun act(a: String, code: Int) = PendingIntent.getService(this, code, Intent(this, CallService::class.java).setAction(a), flags)
    val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), flags)

    val status = when {
      deafened -> "Deafened"
      muted -> "Muted"
      else -> "Connected"
    }
    val n = NotificationCompat.Builder(this, CHANNEL)
      .setSmallIcon(R.drawable.ic_call_note)
      .setContentTitle(room)
      .setContentText(status)
      .setContentIntent(open)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setSilent(true)
      .setCategory(NotificationCompat.CATEGORY_CALL)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .setUsesChronometer(true)
      .setWhen(started)
      .setDeleteIntent(act(DISMISSED, 4))
      .setColor(0xFFF4B860.toInt())
      // Three buttons: a call style would only fit Hang up and one more.
      .addAction(0, if (muted) "Unmute" else "Mute", act("mute", 1))
      .addAction(0, if (deafened) "Undeafen" else "Deafen", act("deafen", 2))
      .addAction(0, "End call", act("leave", 3))
      .build()
    val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
    ServiceCompat.startForeground(this, ID, n, type)
  }

  private val started = System.currentTimeMillis()

  override fun onDestroy() {
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    super.onDestroy()
  }
}
