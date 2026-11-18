package id.hypertopia.rynuvo

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import androidx.core.app.RemoteInput
import android.content.BroadcastReceiver
import androidx.core.graphics.drawable.IconCompat
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Typeface
import java.io.File
import android.widget.RemoteViews
import androidx.core.content.FileProvider
import org.json.JSONArray
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Notifications while Rynuvo is closed. A push from Firebase carries only an id; this fetches what it
 * is about from the Rynuvo server (with the phone's push key, which can read notifications and nothing
 * else) and shows it, so what was said never passes through Google.
 *
 * One notification per chat (a conversation that grows), and one per room invite with a Join button.
 * Nothing shows while the app is on screen: the app shows it itself.
 */
class PushService : FirebaseMessagingService() {
  companion object {
    private const val PREFS = "push"
    private const val CH_MESSAGES = "messages"
    private const val CH_INVITES = "invites"
    private const val CH_ROOMS = "room_activity"
    /** Every notification of ours has a tag starting with this (so the call one is never touched). */
    private const val TAG = "rv:"
    const val EXTRA_CHANNEL = "rynuvo.channel"
    const val REPLY = "reply"

    /** The app is on screen (set by MainActivity). */
    @Volatile var foreground = false

    /** A new Firebase token came; the page signs the phone up with it. */
    @Volatile var onToken: ((String) -> Unit)? = null

    fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /** Where to fetch notifications from, and with what key (from the page, once signed up). */
    fun setup(ctx: Context, server: String, key: String) =
      prefs(ctx).edit().putString("server", server).putString("key", key).apply()

    fun clearSetup(ctx: Context) = prefs(ctx).edit().remove("server").remove("key").apply()

    /** My reply, added to the chat's notification (quietly), which also ends its "sending" state. */
    fun showReply(ctx: Context, channel: Long, text: String) {
      val tag = "${TAG}ch:$channel"
      val nm = ctx.getSystemService(NotificationManager::class.java) ?: return
      val sbn = runCatching { nm.activeNotifications.firstOrNull { it.tag == tag } }.getOrNull() ?: return
      val style = NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(sbn.notification) ?: return
      style.addMessage(text, System.currentTimeMillis(), null as Person?)
      val b = NotificationCompat.Builder(ctx, sbn.notification)
        .setStyle(style)
        .setSilent(true)
        .setOnlyAlertOnce(true)
      runCatching { NotificationManagerCompat.from(ctx).notify(tag, 0, b.build()) }
    }

    /** The app came to the front: what the notifications said is on screen now. */
    fun clearAll(ctx: Context) {
      val nm = ctx.getSystemService(NotificationManager::class.java) ?: return
      runCatching { nm.activeNotifications.filter { it.tag?.startsWith(TAG) == true }.forEach { nm.cancel(it.tag, it.id) } }
    }
  }

  override fun onNewToken(token: String) {
    prefs(this).edit().putString("token", token).apply()
    onToken?.invoke(token)
  }

  override fun onMessageReceived(msg: RemoteMessage) {
    if (foreground) return
    val kind = msg.data["k"] ?: "message"
    // The room emptied: its "someone's in here" goes (no note to fetch for this one).
    if (kind == "roomEnded") {
      msg.data["c"]?.let { c -> runCatching { NotificationManagerCompat.from(this).cancel("${TAG}room:$c", 0) } }
      return
    }
    val id = msg.data["n"] ?: return
    val p = prefs(this)
    val server = p.getString("server", null) ?: return
    val key = p.getString("key", null) ?: return
    val note = fetch(server, key, id)
    if (!NotificationManagerCompat.from(this).areNotificationsEnabled()) return
    ensureChannels()
    when {
      note == null -> plain(kind)
      note.optString("kind") == "invite" -> invite(note, face(note, server, key))
      note.optString("kind") == "room" -> room(note, server, key)
      else -> message(note, face(note, server, key), picture(note, server, key))
    }
  }

  private fun fetch(server: String, key: String, id: String): JSONObject? = runCatching {
    val c = URL("$server/api/v1/push/notes/$id").openConnection() as HttpURLConnection
    c.connectTimeout = 8000
    c.readTimeout = 8000
    c.setRequestProperty("x-push-key", key)
    try {
      if (c.responseCode != 200) return@runCatching null
      JSONObject(c.inputStream.bufferedReader().use { it.readText() })
    } finally {
      c.disconnect()
    }
  }.getOrNull()

  /** A file from the server (with the push key), kept in the cache after the first time. */
  private fun cached(dir: String, name: String, path: String, server: String, key: String): File? = Faces.cached(this, dir, name, path, server, key)

  /** A message's picture, shared with the notification shade (which shows it in the conversation). */
  private fun picture(n: JSONObject, server: String, key: String): android.net.Uri? {
    val id = n.optString("image").takeIf { it.isNotEmpty() && it != "null" } ?: return null
    val f = cached("pictures", id, "/push/media/$id", server, key) ?: return null
    return runCatching {
      val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", f)
      grantUriPermission("com.android.systemui", uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
      uri
    }.getOrNull()
  }

  /** One person's face (photo or initial), by their details in a note. */
  private fun faceOf(p: JSONObject, @Suppress("UNUSED_PARAMETER") server: String, @Suppress("UNUSED_PARAMETER") key: String): Bitmap = Faces.of(this, p)

  /** Faces side by side, each over the last, ringed in the lamp's amber; "+N" past four. */
  private fun pile(people: JSONArray, server: String, key: String): Bitmap {
    val size = 132
    val step = (size * 0.68f).toInt()
    val ring = 8f
    val shown = minOf(people.length(), 4)
    val extra = people.length() - shown
    val count = shown + if (extra > 0) 1 else 0
    val out = Bitmap.createBitmap(size + step * (count - 1).coerceAtLeast(0), size, Bitmap.Config.ARGB_8888)
    val c = Canvas(out)
    val amber = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F4B860") }
    for (i in 0 until shown) {
      val x = (i * step).toFloat()
      c.drawOval(RectF(x, 0f, x + size, size.toFloat()), amber)
      val f = Bitmap.createScaledBitmap(faceOf(people.getJSONObject(i), server, key), (size - ring * 2).toInt(), (size - ring * 2).toInt(), true)
      c.drawBitmap(f, x + ring, ring, null)
    }
    if (extra > 0) {
      val x = (shown * step).toFloat()
      c.drawOval(RectF(x, 0f, x + size, size.toFloat()), amber)
      c.drawOval(RectF(x + ring, ring, x + size - ring, size - ring), Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#2A2233") })
      val t = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.parseColor("#F4B860"); textSize = size * 0.36f; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD); textAlign = Paint.Align.CENTER
      }
      c.drawText("+$extra", x + size / 2f, size / 2f - (t.descent() + t.ascent()) / 2, t)
    }
    return out
  }

  /** The room as a little stage, like in the app: Rynuvo's dark, the lamp's warm pool of light, gold
   *  dust, everyone seated with their name under them (the one who just came in lit up), the room's
   *  name and a LIVE pill. */
  private fun stage(people: JSONArray, room: String, server: String, key: String): Bitmap {
    val w = 1000
    val h = 440
    val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    val c = Canvas(out)
    val r = 44f
    val card = RectF(0f, 0f, w.toFloat(), h.toFloat())
    c.save()
    c.clipPath(android.graphics.Path().apply { addRoundRect(card, r, r, android.graphics.Path.Direction.CW) })
    // The room.
    c.drawRect(card, Paint().apply {
      shader = android.graphics.LinearGradient(0f, 0f, 0f, h.toFloat(), Color.parseColor("#241B30"), Color.parseColor("#15111B"), android.graphics.Shader.TileMode.CLAMP)
    })
    // The lamp: a warm pool on the floor.
    c.drawRect(card, Paint().apply {
      shader = android.graphics.RadialGradient(w / 2f, h * 0.98f, w * 0.62f, intArrayOf(Color.argb(120, 244, 184, 96), Color.argb(34, 244, 184, 96), Color.TRANSPARENT), floatArrayOf(0f, 0.55f, 1f), android.graphics.Shader.TileMode.CLAMP)
    })
    // Gold dust, the same every time for this room.
    val rand = java.util.Random(room.hashCode().toLong())
    val dust = Paint(Paint.ANTI_ALIAS_FLAG)
    repeat(46) {
      val lilac = rand.nextInt(3) == 0
      dust.color = if (lilac) Color.argb(110 + rand.nextInt(100), 196, 181, 255) else Color.argb(120 + rand.nextInt(120), 244, 184, 96)
      c.drawCircle(rand.nextFloat() * w, rand.nextFloat() * h, 1.6f + rand.nextFloat() * 3.4f, dust)
    }
    // The room's name, with a speaker.
    val name = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; textSize = 46f; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD) }
    val amberStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F4B860"); style = Paint.Style.STROKE; strokeWidth = 6f; strokeCap = Paint.Cap.ROUND }
    val amber = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F4B860") }
    val sx = 44f
    val sy = 66f
    c.drawPath(android.graphics.Path().apply {
      moveTo(sx, sy - 8); lineTo(sx + 12, sy - 8); lineTo(sx + 26, sy - 22); lineTo(sx + 26, sy + 22); lineTo(sx + 12, sy + 8); lineTo(sx, sy + 8); close()
    }, amber)
    c.drawArc(RectF(sx + 18, sy - 16, sx + 46, sy + 16), -50f, 100f, false, amberStroke)
    c.drawArc(RectF(sx + 16, sy - 28, sx + 62, sy + 28), -50f, 100f, false, amberStroke)
    val title = android.text.TextUtils.ellipsize(room, android.text.TextPaint(name), w - 360f, android.text.TextUtils.TruncateAt.END).toString()
    c.drawText(title, sx + 84, sy + 16, name)
    // LIVE.
    val live = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; textSize = 30f; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD); letterSpacing = 0.12f }
    val lw = live.measureText("LIVE")
    val pill = RectF(w - 44f - lw - 74, sy - 26, w - 44f, sy + 26)
    c.drawRoundRect(pill, 26f, 26f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(70, 239, 68, 68) })
    c.drawCircle(pill.left + 30, sy, 9f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F87171") })
    c.drawText("LIVE", pill.left + 52, sy + 11, live)
    // Everyone, seated round the lamp.
    val shown = minOf(people.length(), 5)
    val extra = people.length() - shown
    val seats = shown + if (extra > 0) 1 else 0
    val size = if (seats <= 3) 150 else 124
    val gap = if (seats <= 3) 64 else 40
    val total = seats * size + (seats - 1) * gap
    var x = (w - total) / 2f
    val top = 138f
    val label = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(225, 255, 255, 255); textSize = 30f; textAlign = Paint.Align.CENTER; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD) }
    for (i in 0 until shown) {
      val p = people.getJSONObject(i)
      val cx = x + size / 2f
      val cy = top + size / 2f
      if (i == 0) {
        // Just came in: lit up.
        c.drawCircle(cx, cy, size * 0.66f, Paint(Paint.ANTI_ALIAS_FLAG).apply {
          shader = android.graphics.RadialGradient(cx, cy, size * 0.66f, intArrayOf(Color.argb(150, 244, 184, 96), Color.TRANSPARENT), null, android.graphics.Shader.TileMode.CLAMP)
        })
      }
      c.drawCircle(cx, cy, size / 2f, if (i == 0) amber else Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(90, 255, 255, 255) })
      val ring = if (i == 0) 8 else 5
      val f = Bitmap.createScaledBitmap(faceOf(p, server, key), size - ring * 2, size - ring * 2, true)
      c.drawBitmap(f, x + ring, top + ring, null)
      val nm = android.text.TextUtils.ellipsize(p.optString("name"), android.text.TextPaint(label), size + gap - 12f, android.text.TextUtils.TruncateAt.END).toString()
      c.drawText(nm, cx, top + size + 44, label)
      x += size + gap
    }
    if (extra > 0) {
      val cx = x + size / 2f
      val cy = top + size / 2f
      c.drawCircle(cx, cy, size / 2f, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(60, 255, 255, 255) })
      val t = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; textSize = size * 0.32f; textAlign = Paint.Align.CENTER; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD) }
      c.drawText("+$extra", cx, cy - (t.descent() + t.ascent()) / 2, t)
    }
    c.restore()
    return out
  }

  /** "Mei is in Music Room": the room's stage, who's there, and Join. Updated in place as more come in. */
  private fun room(n: JSONObject, server: String, key: String) {
    val channel = n.optLong("channelId")
    val people = n.optJSONArray("people") ?: JSONArray()
    if (people.length() == 0) return
    val room = n.optString("channel")
    val first = people.getJSONObject(0).optString("name")
    val who = when (people.length()) {
      1 -> "$first is hanging out. Hop in?"
      2 -> "$first joined ${people.getJSONObject(1).optString("name")}. Hop in?"
      else -> "$first joined ${people.getJSONObject(1).optString("name")} and ${people.length() - 2} more. Hop in?"
    }
    val join = open(channel)
    val small = RemoteViews(packageName, R.layout.note_room).apply {
      setImageViewBitmap(R.id.faces, pile(people, server, key))
      setTextViewText(R.id.title, room)
      setTextViewText(R.id.sub, if (people.length() == 1) "$first is hanging out" else "$first and ${people.length() - 1} more")
      setOnClickPendingIntent(R.id.join, join)
    }
    val big = RemoteViews(packageName, R.layout.note_room_big).apply {
      setImageViewBitmap(R.id.stage, stage(people, room, server, key))
      setTextViewText(R.id.sub, who)
      setOnClickPendingIntent(R.id.join, join)
    }
    val b = NotificationCompat.Builder(this, CH_ROOMS)
      .setSmallIcon(R.drawable.ic_call_note)
      .setColor(0xFFF4B860.toInt())
      .setStyle(NotificationCompat.DecoratedCustomViewStyle())
      .setCustomContentView(small)
      .setCustomHeadsUpContentView(small)
      .setCustomBigContentView(big)
      .setContentTitle("$first is in $room")
      .setContentText(who)
      .setContentIntent(join)
      .setAutoCancel(true)
      .setOnlyAlertOnce(true)
      // This hangout already rang this phone: later arrivals only update it.
      .setSilent(n.optBoolean("quiet"))
      .setCategory(NotificationCompat.CATEGORY_SOCIAL)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
    runCatching { NotificationManagerCompat.from(this).notify("${TAG}room:$channel", 0, b.build()) }
  }

  /** The sender's face: their profile photo, or their initial on their avatar colour, like in the app. */
  private fun face(n: JSONObject, server: String, key: String): Bitmap = faceOf(
    JSONObject().put("id", n.optLong("fromId")).put("name", n.optString("from"))
      .put("avatar", n.opt("avatar") ?: JSONObject.NULL).put("color", n.opt("color") ?: JSONObject.NULL),
    server, key,
  )

  /** A chat: the amber # on Rynuvo's dark, as in the app's list. */
  private fun chatIcon(): Bitmap {
    val size = 192
    val out = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
    val c = Canvas(out)
    c.drawOval(RectF(0f, 0f, size.toFloat(), size.toFloat()), Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#2A2233") })
    val t = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.parseColor("#F4B860")
      textSize = size * 0.56f
      typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
      textAlign = Paint.Align.CENTER
    }
    c.drawText("#", size / 2f, size / 2f - (t.descent() + t.ascent()) / 2, t)
    return out
  }

  private fun ensureChannels() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val nm = getSystemService(NotificationManager::class.java)
    if (nm.getNotificationChannel(CH_MESSAGES) == null) {
      nm.createNotificationChannel(NotificationChannel(CH_MESSAGES, "Messages", NotificationManager.IMPORTANCE_HIGH).apply {
        description = "New messages while Rynuvo is closed"
      })
    }
    if (nm.getNotificationChannel(CH_ROOMS) == null) {
      nm.deleteNotificationChannel("rooms") // the first one, which didn't pop up
      nm.createNotificationChannel(NotificationChannel(CH_ROOMS, "Room activity", NotificationManager.IMPORTANCE_HIGH).apply {
        description = "Friends hanging out in a voice room"
      })
    }
    if (nm.getNotificationChannel(CH_INVITES) == null) {
      nm.createNotificationChannel(NotificationChannel(CH_INVITES, "Room invites", NotificationManager.IMPORTANCE_HIGH).apply {
        description = "Friends asking you to join their room"
      })
    }
  }

  /** Opens the chat or room (a room is joined on the way in). */
  private fun open(channel: Long): PendingIntent {
    val i = Intent(this, MainActivity::class.java)
      .putExtra(EXTRA_CHANNEL, channel)
      .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    return PendingIntent.getActivity(this, channel.toInt(), i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
  }

  private fun message(n: JSONObject, face: Bitmap?, picture: android.net.Uri?) {
    val channel = n.optLong("channelId")
    val tag = "${TAG}ch:$channel"
    val nm = getSystemService(NotificationManager::class.java)
    val me = Person.Builder().setName("You").build()
    // Grow the chat's notification instead of stacking a new one per message.
    val earlier = runCatching { nm.activeNotifications.firstOrNull { it.tag == tag }?.notification }.getOrNull()
    val style = earlier?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) }
      ?: NotificationCompat.MessagingStyle(me)
    style.setConversationTitle("#${n.optString("channel")}").setGroupConversation(true)
    val from = Person.Builder().setName(n.optString("from")).setKey("u${n.optLong("fromId")}")
      .apply { face?.let { setIcon(IconCompat.createWithBitmap(it)) } }.build()
    val caption = n.optString("text").takeIf { it.isNotEmpty() && it != "sent an image" }
    val m = NotificationCompat.MessagingStyle.Message(caption ?: if (picture != null) "Photo" else "sent something", n.optLong("at", System.currentTimeMillis()), from)
    // The picture itself, in the conversation (the shade shows it; tapping it opens the chat).
    if (picture != null) m.setData("image/", picture)
    style.addMessage(m)
    val b = NotificationCompat.Builder(this, CH_MESSAGES)
      .setSmallIcon(R.drawable.ic_call_note)
      .setColor(0xFFF4B860.toInt())
      .setStyle(style)
      // The chat's own picture (a # like in the app); the sender's face sits by their message. Without
      // one Android piles up faces with a grey stand-in for the group.
      .setLargeIcon(chatIcon())
      .setContentIntent(open(channel))
      .setAutoCancel(true)
      .setCategory(NotificationCompat.CATEGORY_MESSAGE)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
    if (n.optBoolean("mention")) b.setSubText("Mentioned you")
    // Reply right from the notification.
    val reply = PendingIntent.getBroadcast(
      this, channel.toInt(),
      Intent(this, ReplyReceiver::class.java).putExtra(EXTRA_CHANNEL, channel),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
    )
    b.addAction(
      NotificationCompat.Action.Builder(0, "Reply", reply)
        .addRemoteInput(RemoteInput.Builder(REPLY).setLabel("Message #${n.optString("channel")}").build())
        .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
        .setShowsUserInterface(false)
        .build(),
    )
    runCatching { NotificationManagerCompat.from(this).notify(tag, 0, b.build()) }
  }

  private fun invite(n: JSONObject, face: Bitmap?) {
    val channel = n.optLong("channelId")
    val b = NotificationCompat.Builder(this, CH_INVITES)
      .setSmallIcon(R.drawable.ic_call_note)
      .setColor(0xFFF4B860.toInt())
      .setContentTitle("${n.optString("from")} wants you in ${n.optString("channel")}")
      .setContentText("Come hang out. Tap to join.")
      .setLargeIcon(face)
      .setContentIntent(open(channel))
      .addAction(0, "Join", open(channel))
      .setAutoCancel(true)
      // An invite is for now: it goes away by itself.
      .setTimeoutAfter(2 * 60_000L)
      .setCategory(NotificationCompat.CATEGORY_SOCIAL)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
    runCatching { NotificationManagerCompat.from(this).notify("${TAG}inv:$channel", 0, b.build()) }
  }

  /** The server couldn't be reached for the details: still say something came. */
  private fun plain(kind: String) {
    val launch = packageManager.getLaunchIntentForPackage(packageName)
      ?.let { PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE) }
    val b = NotificationCompat.Builder(this, if (kind == "invite") CH_INVITES else CH_MESSAGES)
      .setSmallIcon(R.drawable.ic_call_note)
      .setColor(0xFFF4B860.toInt())
      .setContentTitle("Rynuvo")
      .setContentText(if (kind == "invite") "A friend wants you in their room" else "You have new messages")
      .setContentIntent(launch)
      .setAutoCancel(true)
    runCatching { NotificationManagerCompat.from(this).notify("${TAG}plain", 0, b.build()) }
  }
}

/**
 * A reply typed into a message notification: sent to the server as this phone's user (with the push
 * key), then shown in the conversation as mine so the notification stops waiting.
 */
class ReplyReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    val text = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(PushService.REPLY)?.toString()?.trim()
    val channel = intent.getLongExtra(PushService.EXTRA_CHANNEL, 0)
    if (text.isNullOrEmpty() || channel == 0L) return
    val done = goAsync()
    Thread {
      val p = PushService.prefs(ctx)
      val server = p.getString("server", null)
      val key = p.getString("key", null)
      val sent = server != null && key != null && runCatching {
        val c = URL("$server/api/v1/push/reply").openConnection() as HttpURLConnection
        c.connectTimeout = 8000
        c.readTimeout = 8000
        c.requestMethod = "POST"
        c.doOutput = true
        c.setRequestProperty("x-push-key", key)
        c.setRequestProperty("content-type", "application/json")
        c.outputStream.use { it.write(JSONObject().put("channelId", channel).put("content", text).toString().toByteArray()) }
        try { c.responseCode in 200..299 } finally { c.disconnect() }
      }.getOrDefault(false)
      PushService.showReply(ctx, channel, if (sent) text else "Couldn't send: $text")
      done.finish()
    }.start()
  }
}
