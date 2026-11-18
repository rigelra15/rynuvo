package id.hypertopia.rynuvo

import android.content.Context
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
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONObject

/** People's faces, drawn the way the app draws them: their photo in a circle, or their initial on
 *  their avatar colour. Used by notifications and the call bubble. Photos are fetched with the
 *  push key and kept in the cache, so this may touch the network: call it off the main thread. */
object Faces {
  /** One person by `{id, name, avatar, color}`. */
  fun of(ctx: Context, p: JSONObject): Bitmap {
    val photo = p.optString("avatar").takeIf { it.isNotEmpty() && it != "null" }
    val prefs = PushService.prefs(ctx)
    val server = prefs.getString("server", null)
    val key = prefs.getString("key", null)
    if (photo != null && server != null && key != null) {
      val f = cached(ctx, "faces", photo, "/push/avatar/${p.optLong("id")}", server, key)
      val bmp = f?.let { runCatching { decodeSmall(it) }.getOrNull() }
      if (bmp != null) return round(bmp)
    }
    val color = runCatching { Color.parseColor(p.optString("color")) }.getOrNull() ?: Color.parseColor("#8B7FD1")
    return initial(p.optString("name"), color)
  }

  /** A file from the server (with the push key), kept in the cache after the first time. */
  fun cached(ctx: Context, dir: String, name: String, path: String, server: String, key: String): File? {
    val d = File(ctx.cacheDir, dir).apply { mkdirs() }
    val f = File(d, name.filter { it.isLetterOrDigit() })
    if (f.exists()) return f
    return runCatching {
      val c = URL("$server/api/v1$path").openConnection() as HttpURLConnection
      c.connectTimeout = 8000
      c.readTimeout = 10000
      c.setRequestProperty("x-push-key", key)
      try {
        if (c.responseCode != 200) return@runCatching null
        val tmp = File(d, "${f.name}.part")
        c.inputStream.use { inp -> tmp.outputStream().use { inp.copyTo(it) } }
        tmp.renameTo(f)
        f
      } finally {
        c.disconnect()
      }
    }.getOrNull()
  }

  fun decodeSmall(f: File): Bitmap? {
    val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(f.path, o)
    var s = 1
    while (o.outWidth / (s * 2) >= 192 && o.outHeight / (s * 2) >= 192) s *= 2
    return BitmapFactory.decodeFile(f.path, BitmapFactory.Options().apply { inSampleSize = s })
  }

  /** Centre-cropped into a circle. */
  fun round(src: Bitmap): Bitmap {
    val size = minOf(src.width, src.height, 192)
    val out = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
    val c = Canvas(out)
    val p = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    c.drawOval(RectF(0f, 0f, size.toFloat(), size.toFloat()), p)
    p.xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC_IN)
    val side = minOf(src.width, src.height)
    val from = Rect((src.width - side) / 2, (src.height - side) / 2, (src.width + side) / 2, (src.height + side) / 2)
    c.drawBitmap(src, from, Rect(0, 0, size, size), p)
    return out
  }

  fun initial(name: String, color: Int): Bitmap {
    val size = 192
    val out = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
    val c = Canvas(out)
    c.drawOval(RectF(0f, 0f, size.toFloat(), size.toFloat()), Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color })
    val letter = name.trim().take(1).uppercase()
    val dark = (Color.red(color) * 299 + Color.green(color) * 587 + Color.blue(color) * 114) / 1000 > 150
    val t = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      this.color = if (dark) Color.parseColor("#2A1A05") else Color.WHITE
      textSize = size * 0.46f
      typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
      textAlign = Paint.Align.CENTER
    }
    c.drawText(letter, size / 2f, size / 2f - (t.descent() + t.ascent()) / 2, t)
    return out
  }
}
