package id.hypertopia.rynuvo

import android.animation.ValueAnimator
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.animation.DecelerateInterpolator
import android.view.animation.OvershootInterpolator
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import java.util.concurrent.Executors
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import org.json.JSONObject

/**
 * The call, floating over other apps while Rynuvo is in the background: a round bubble with the face
 * of whoever is talking (ringed in their colour while they talk), that can be dragged, sticks to the
 * side of the screen, and goes when dropped on the ✕. A tap opens a small card with everyone in the
 * room and Mute, Deafen, Open Rynuvo and Leave.
 *
 * The page sends the call's state ([update], [speaking]); the activity says when it's on screen
 * ([appVisible]). Buttons go back to the page the way the call notification's do.
 */
object Bubble {
  private const val PREFS = "bubble"
  private val main = Handler(Looper.getMainLooper())
  private val loader = Executors.newSingleThreadExecutor()

  // From the page
  private var state: JSONObject? = null
  private var talking: Set<Long> = emptySet()
  /** The last one heard: the bubble keeps their face a moment after they stop. */
  private var lastTalker = 0L
  private var appShown = true
  /** Dropped on the ✕: gone until Rynuvo is opened again (or the next call). */
  private var dismissedRoom: String? = null

  // On screen
  private var wm: WindowManager? = null
  private var ctx: Context? = null
  private var bubble: BubbleView? = null
  private var bubbleLp: WindowManager.LayoutParams? = null
  private var target: TargetView? = null
  private var panel: View? = null
  private var panelClosedAt = 0L
  private var refreshPanel: (() -> Unit)? = null

  private val faces = HashMap<String, Bitmap>()
  private val loading = HashSet<String>()

  // ---- what the page and activity call -------------------------------------------------------

  /** Whether the user wants the bubble (on unless turned off). */
  fun enabled(c: Context) = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("on", true)

  fun setEnabled(c: Context, on: Boolean) {
    c.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("on", on).apply()
    sync(c)
  }

  fun allowed(c: Context) = Settings.canDrawOverlays(c)

  /** "on", "off" (turned off here), or "permission" (Android's "display over other apps" is off). */
  fun status(c: Context) = when {
    !allowed(c) -> "permission"
    !enabled(c) -> "off"
    else -> "on"
  }

  /** Android's page for "display over other apps", for Rynuvo. */
  fun askPermission(c: Context) {
    runCatching {
      c.startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${c.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
  }

  /** The call: `{room, muted, deafened, me, people: [{id, name, avatar, color, muted, deafened}]}`. */
  fun update(c: Context, json: String) = main.post {
    val s = runCatching { JSONObject(json) }.getOrNull() ?: return@post
    if (state?.optString("room") != s.optString("room")) dismissedRoom = null
    state = s
    sync(c)
    bubble?.invalidateFace()
    refreshPanel?.invoke()
  }

  fun speaking(ids: String) = main.post {
    talking = ids.split(',').mapNotNull { it.trim().toLongOrNull() }.toSet()
    talking.firstOrNull()?.let { lastTalker = it }
    bubble?.invalidateFace()
    refreshPanel?.invoke()
  }

  /** The call ended. */
  fun end(c: Context) = main.post {
    state = null
    talking = emptySet()
    lastTalker = 0
    dismissedRoom = null
    sync(c)
  }

  fun appVisible(c: Context, shown: Boolean) = main.post {
    appShown = shown
    // Put away on the ✕, it comes back the next time Rynuvo is left with the call on.
    if (shown) dismissedRoom = null
    sync(c)
  }

  // ---- showing and hiding ----------------------------------------------------------------------

  private fun sync(c: Context) {
    val s = state
    val want = s != null && !appShown && enabled(c) && allowed(c) && dismissedRoom != s.optString("room")
    if (want && bubble == null) show(c.applicationContext) else if (!want && bubble != null) hide()
  }

  private fun overlay(w: Int, h: Int, flags: Int) = WindowManager.LayoutParams(
    w, h, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY, flags, PixelFormat.TRANSLUCENT,
  ).apply { gravity = Gravity.TOP or Gravity.START }

  private fun show(c: Context) {
    ctx = c
    val w = c.getSystemService(WindowManager::class.java)
    wm = w
    val size = dp(c, 72)
    // The ✕ goes in first, hidden, so the bubble is above it (adding it mid-drag would cancel the drag).
    addTarget(c)
    val v = BubbleView(c)
    val lp = overlay(size, size, WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS)
    val prefs = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val (sw, sh) = screen(c)
    lp.x = if (prefs.getBoolean("left", false)) -dp(c, 6) else sw - size + dp(c, 6)
    lp.y = prefs.getInt("y", (sh * 0.3f).toInt()).coerceIn(dp(c, 24), sh - size - dp(c, 24))
    runCatching { w.addView(v, lp) }.onFailure { hide(); return }
    bubble = v
    bubbleLp = lp
    v.scaleX = 0f; v.scaleY = 0f
    v.animate().scaleX(1f).scaleY(1f).setDuration(260).setInterpolator(OvershootInterpolator()).start()
  }

  private fun hide() {
    closePanel()
    target?.let { t -> runCatching { wm?.removeView(t) } }
    target = null
    bubble?.let { b -> b.stop(); runCatching { wm?.removeView(b) } }
    bubble = null
    bubbleLp = null
  }

  private fun screen(c: Context): Pair<Int, Int> {
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
      val b = c.getSystemService(WindowManager::class.java).currentWindowMetrics.bounds
      return b.width() to b.height()
    }
    val m = android.util.DisplayMetrics()
    @Suppress("DEPRECATION") c.getSystemService(WindowManager::class.java).defaultDisplay.getRealMetrics(m)
    return m.widthPixels to m.heightPixels
  }

  private fun dp(c: Context, v: Int) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), c.resources.displayMetrics).toInt()

  // ---- faces -----------------------------------------------------------------------------------

  private fun person(id: Long): JSONObject? {
    val ps = state?.optJSONArray("people") ?: return null
    for (i in 0 until ps.length()) ps.optJSONObject(i)?.let { if (it.optLong("id") == id) return it }
    return null
  }

  private fun people(): List<JSONObject> {
    val ps = state?.optJSONArray("people") ?: return emptyList()
    return (0 until ps.length()).mapNotNull { ps.optJSONObject(it) }
  }

  /** Their face if it's ready; otherwise it's fetched and everything showing redraws once it is. */
  private fun face(p: JSONObject): Bitmap? {
    val c = ctx ?: return null
    val key = "${p.optLong("id")}:${p.optString("avatar")}:${p.optString("color")}:${p.optString("name")}"
    faces[key]?.let { return it }
    if (loading.add(key)) loader.execute {
      val b = runCatching { Faces.of(c, p) }.getOrNull()
      main.post {
        loading.remove(key)
        if (b != null) faces[key] = b
        bubble?.invalidateFace()
        refreshPanel?.invoke()
      }
    }
    return null
  }

  private fun colorOf(p: JSONObject?) = runCatching { Color.parseColor(p?.optString("color")) }.getOrNull() ?: Color.parseColor("#F4B860")

  // ---- the bubble ------------------------------------------------------------------------------

  @SuppressLint("ViewConstructor")
  private class BubbleView(c: Context) : View(c) {
    private val pFace = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val pRing = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val pShadow = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.TRANSPARENT; setShadowLayer(14f, 0f, 4f, 0x66000000) }
    private val pBase = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#2A2233") }
    private val pBadge = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F2545B") }
    private val pCount = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#1F1927") }
    private val pCountText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.parseColor("#F4B860"); typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD); textAlign = Paint.Align.CENTER
    }
    // White on the red badge: the badge itself says "off".
    private val micMark = c.getDrawable(R.drawable.ic_b_mic)!!.mutate()
    private val headMark = c.getDrawable(R.drawable.ic_b_head)!!.mutate()
    private var pulse = 0f
    private val anim = ValueAnimator.ofFloat(0f, 1f).apply {
      duration = 1100
      repeatCount = ValueAnimator.INFINITE
      addUpdateListener { pulse = it.animatedValue as Float; if (talkingNow()) invalidate() }
      start()
    }

    init {
      setLayerType(LAYER_TYPE_SOFTWARE, null) // for the shadow
      setOnTouchListener(Drag(this))
    }

    fun stop() = anim.cancel()

    fun invalidateFace() = invalidate()

    private fun talkingNow() = talking.isNotEmpty()

    /** Whose face: the one talking, else the last one heard (if still here), else me. */
    private fun who(): JSONObject? {
      talking.firstOrNull()?.let { id -> person(id)?.let { return it } }
      person(lastTalker)?.let { return it }
      return state?.optLong("me")?.let { person(it) } ?: people().firstOrNull()
    }

    override fun onDraw(cv: Canvas) {
      val w = width.toFloat()
      val cx = w / 2
      val r = w * 0.36f
      val p = who()
      cv.drawCircle(cx, cx, r, pShadow)
      if (talkingNow()) {
        val col = colorOf(p)
        pRing.color = col
        // An outer ring that breathes out, and a steady one round the face.
        pRing.strokeWidth = w * 0.035f
        pRing.alpha = ((1f - pulse) * 170).toInt()
        cv.drawCircle(cx, cx, r + w * 0.04f + pulse * w * 0.08f, pRing)
        pRing.alpha = 255
        pRing.strokeWidth = w * 0.045f
        cv.drawCircle(cx, cx, r + w * 0.02f, pRing)
      }
      cv.drawCircle(cx, cx, r, pBase)
      val f = p?.let { face(it) }
      if (f != null) cv.drawBitmap(f, null, RectF(cx - r, cx - r, cx + r, cx + r), pFace)
      // How many are here, top right, when it's more than just me and one other.
      val n = people().size
      if (n > 2) {
        val br = w * 0.13f
        val bx = cx + r * 0.78f; val by = cx - r * 0.78f
        cv.drawCircle(bx, by, br, pCount)
        pCountText.textSize = br * 1.15f
        cv.drawText(if (n > 99) "99" else "$n", bx, by - (pCountText.descent() + pCountText.ascent()) / 2, pCountText)
      }
      // Muted or deafened, bottom right.
      val deaf = state?.optBoolean("deafened") == true
      val muted = state?.optBoolean("muted") == true
      if (deaf || muted) {
        val br = w * 0.14f
        val bx = cx + r * 0.76f; val by = cx + r * 0.76f
        cv.drawCircle(bx, by, br, pBadge)
        val d = if (deaf) headMark else micMark
        val s = (br * 1.25f).toInt()
        d.setBounds((bx - s / 2).toInt(), (by - s / 2).toInt(), (bx + s / 2).toInt(), (by + s / 2).toInt())
        d.draw(cv)
      }
    }
  }

  /** Dragging the bubble about; a tap opens the card; dropped on the ✕ it goes. */
  private class Drag(private val v: View) : View.OnTouchListener {
    private var downX = 0f; private var downY = 0f
    private var startX = 0; private var startY = 0
    private var moved = false
    private val slop = ViewConfiguration.get(v.context).scaledTouchSlop

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouch(view: View, e: MotionEvent): Boolean {
      val lp = bubbleLp ?: return false
      val c = view.context
      when (e.actionMasked) {
        MotionEvent.ACTION_DOWN -> {
          downX = e.rawX; downY = e.rawY; startX = lp.x; startY = lp.y; moved = false
          v.animate().scaleX(0.92f).scaleY(0.92f).setDuration(90).start()
        }
        MotionEvent.ACTION_MOVE -> {
          val dx = e.rawX - downX; val dy = e.rawY - downY
          if (!moved && hypot(dx, dy) > slop) {
            moved = true
            closePanel()
            showTarget(c)
          }
          if (moved) {
            lp.x = (startX + dx).toInt(); lp.y = (startY + dy).toInt()
            val t = target
            if (t != null && t.near(lp.x + v.width / 2, lp.y + v.height / 2)) {
              // Pulled onto the ✕.
              lp.x = t.cx() - v.width / 2; lp.y = t.cy() - v.height / 2
              t.hot(true)
            } else t?.hot(false)
            runCatching { wm?.updateViewLayout(v, lp) }
          }
        }
        MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
          v.animate().scaleX(1f).scaleY(1f).setDuration(120).start()
          if (!moved) {
            if (e.actionMasked == MotionEvent.ACTION_UP) togglePanel(c)
            return true
          }
          val t = target
          if (t != null && t.isHot()) {
            dismissedRoom = state?.optString("room")
            v.animate().scaleX(0f).scaleY(0f).setDuration(160).withEndAction { hide() }.start()
            return true
          }
          hideTarget()
          snap(c, v, lp)
        }
      }
      return true
    }
  }

  /** To the nearer side, and remembered for next time. */
  private fun snap(c: Context, v: View, lp: WindowManager.LayoutParams) {
    val (sw, sh) = screen(c)
    val left = lp.x + v.width / 2 < sw / 2
    val toX = if (left) -dp(c, 6) else sw - v.width + dp(c, 6)
    lp.y = lp.y.coerceIn(dp(c, 24), sh - v.height - dp(c, 48))
    val fromX = lp.x
    ValueAnimator.ofFloat(0f, 1f).apply {
      duration = 280
      interpolator = OvershootInterpolator(1.1f)
      addUpdateListener {
        if (bubble !== v) return@addUpdateListener
        lp.x = (fromX + (toX - fromX) * (it.animatedValue as Float)).toInt()
        runCatching { wm?.updateViewLayout(v, lp) }
      }
      start()
    }
    c.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("left", left).putInt("y", lp.y).apply()
  }

  // ---- the ✕ to drop it on -----------------------------------------------------------------------

  @SuppressLint("ViewConstructor")
  private class TargetView(c: Context) : FrameLayout(c) {
    private val ring = View(c)
    private var hotNow = false
    private var cx = 0
    private var cy = 0

    init {
      val s = dp(c, 64)
      ring.background = GradientDrawable().apply {
        shape = GradientDrawable.OVAL
        setColor(Color.parseColor("#E61F1927"))
        setStroke(dp(c, 2), Color.parseColor("#55FFFFFF"))
      }
      addView(ring, LayoutParams(s, s, Gravity.CENTER))
      addView(ImageView(c).apply { setImageResource(R.drawable.ic_b_close) }, LayoutParams(dp(c, 26), dp(c, 26), Gravity.CENTER))
      alpha = 0f
      visibility = INVISIBLE
    }

    fun place(x: Int, y: Int) { cx = x; cy = y }
    fun cx() = cx
    fun cy() = cy
    fun isHot() = hotNow

    fun near(x: Int, y: Int) = hypot((x - cx).toFloat(), (y - cy).toFloat()) < dp(context, if (hotNow) 96 else 80)

    fun hot(on: Boolean) {
      if (on == hotNow) return
      hotNow = on
      if (on) performHapticFeedback(android.view.HapticFeedbackConstants.VIRTUAL_KEY)
      ring.animate().scaleX(if (on) 1.3f else 1f).scaleY(if (on) 1.3f else 1f).setDuration(120).start()
      (ring.background as GradientDrawable).setColor(Color.parseColor(if (on) "#F2545B" else "#E61F1927"))
    }
  }

  private fun addTarget(c: Context) {
    if (target != null) return
    val t = TargetView(c)
    val box = dp(c, 120)
    val (sw, sh) = screen(c)
    val lp = overlay(box, box, WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE)
    lp.x = (sw - box) / 2
    lp.y = sh - box - dp(c, 56)
    t.place(lp.x + box / 2, lp.y + box / 2)
    runCatching { wm?.addView(t, lp) }.onSuccess { target = t }
  }

  @Suppress("UNUSED_PARAMETER")
  private fun showTarget(c: Context) {
    val t = target ?: return
    t.hot(false)
    t.visibility = View.VISIBLE
    t.animate().alpha(1f).setDuration(150).start()
  }

  private fun hideTarget() {
    val t = target ?: return
    t.animate().alpha(0f).setDuration(120).withEndAction { t.visibility = View.INVISIBLE }.start()
  }

  // ---- the card ----------------------------------------------------------------------------------

  private fun togglePanel(c: Context) {
    if (panel != null) return closePanel()
    // The tap that closed it (outside the card, on the bubble) shouldn't open it straight back.
    if (System.currentTimeMillis() - panelClosedAt < 350) return
    openPanel(c)
  }

  private fun closePanel() {
    val p = panel ?: return
    panel = null
    refreshPanel = null
    panelClosedAt = System.currentTimeMillis()
    p.animate().alpha(0f).translationY(-dp(p.context, 8).toFloat()).setDuration(130).withEndAction { runCatching { wm?.removeView(p) } }.start()
  }

  @SuppressLint("ClickableViewAccessibility")
  private fun openPanel(c: Context) {
    val b = bubble ?: return
    val blp = bubbleLp ?: return
    val text = Color.parseColor("#F1EBE4")
    val muted = Color.parseColor("#AA9FB2")

    val card = LinearLayout(c).apply {
      orientation = LinearLayout.VERTICAL
      setPadding(dp(c, 18), dp(c, 16), dp(c, 18), dp(c, 16))
      background = GradientDrawable().apply {
        cornerRadius = dp(c, 24).toFloat()
        setColor(Color.parseColor("#F21F1927"))
        setStroke(dp(c, 1), Color.parseColor("#3A3044"))
      }
      elevation = dp(c, 12).toFloat()
    }
    val title = TextView(c).apply {
      setTextColor(text); textSize = 17f; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
      maxLines = 1; ellipsize = TextUtils.TruncateAt.END
    }
    val sub = TextView(c).apply { setTextColor(muted); textSize = 13f }
    card.addView(title)
    card.addView(sub)

    val row = LinearLayout(c).apply { orientation = LinearLayout.HORIZONTAL }
    val scroll = HorizontalScrollView(c).apply { isHorizontalScrollBarEnabled = false; addView(row) }
    card.addView(scroll, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(c, 14) })

    val controls = LinearLayout(c).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER }
    card.addView(controls, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(c, 16) })

    fun button(icon: Int, label: String, bg: Int, act: () -> Unit): Pair<LinearLayout, ImageView> {
      val img = ImageView(c).apply {
        setImageResource(icon)
        val pad = dp(c, 13)
        setPadding(pad, pad, pad, pad)
        background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(bg) }
        contentDescription = label
      }
      val cap = TextView(c).apply { setTextColor(muted); textSize = 11f; gravity = Gravity.CENTER; this.text = label }
      val col = LinearLayout(c).apply {
        orientation = LinearLayout.VERTICAL; gravity = Gravity.CENTER_HORIZONTAL
        addView(img, LinearLayout.LayoutParams(dp(c, 50), dp(c, 50)))
        addView(cap, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(c, 5) })
        setOnClickListener { act() }
      }
      controls.addView(col, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
      return col to img
    }
    val plain = Color.parseColor("#2E2638")
    val off = Color.parseColor("#4D2A31")
    val (micCol, micImg) = button(R.drawable.ic_b_mic, "Mute", plain) { CallService.onAction?.invoke("mute") }
    val (headCol, headImg) = button(R.drawable.ic_b_head, "Deafen", plain) { CallService.onAction?.invoke("deafen") }
    button(R.drawable.ic_b_open, "Open", Color.parseColor("#3D3226")) {
      closePanel()
      runCatching {
        c.startActivity(Intent(c, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT))
      }
    }
    button(R.drawable.ic_b_leave, "Leave", Color.parseColor("#F2545B")) {
      closePanel()
      CallService.onAction?.invoke("leave")
    }

    fun fill() {
      val s = state ?: return closePanel()
      title.text = s.optString("room", "Call")
      val ps = people()
      val now = talking.mapNotNull { id -> ps.firstOrNull { it.optLong("id") == id }?.optString("name") }
      sub.text = when {
        now.isNotEmpty() -> now.joinToString(", ") + if (now.size == 1) " is talking" else " are talking"
        ps.size == 1 -> "Just you"
        else -> "${ps.size} in the room"
      }
      val isMuted = s.optBoolean("muted")
      val isDeaf = s.optBoolean("deafened")
      micImg.setImageResource(if (isMuted) R.drawable.ic_b_mic_off else R.drawable.ic_b_mic)
      (micImg.background as GradientDrawable).setColor(if (isMuted) off else plain)
      (micCol.getChildAt(1) as TextView).text = if (isMuted) "Unmute" else "Mute"
      headImg.setImageResource(if (isDeaf) R.drawable.ic_b_head_off else R.drawable.ic_b_head)
      (headImg.background as GradientDrawable).setColor(if (isDeaf) off else plain)
      (headCol.getChildAt(1) as TextView).text = if (isDeaf) "Undeafen" else "Deafen"

      row.removeAllViews()
      for (p in ps) {
        val id = p.optLong("id")
        val seat = LinearLayout(c).apply { orientation = LinearLayout.VERTICAL; gravity = Gravity.CENTER_HORIZONTAL }
        seat.addView(SeatFace(c, p, id in talking), LinearLayout.LayoutParams(dp(c, 52), dp(c, 52)))
        seat.addView(TextView(c).apply {
          this.text = if (id == s.optLong("me")) "You" else p.optString("name")
          setTextColor(if (id in talking) text else muted); textSize = 11f
          maxLines = 1; ellipsize = TextUtils.TruncateAt.END; gravity = Gravity.CENTER
        }, LinearLayout.LayoutParams(dp(c, 60), LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(c, 4) })
        row.addView(seat, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { marginEnd = dp(c, 6) })
      }
    }
    fill()
    refreshPanel = { fill() }

    val (sw, sh) = screen(c)
    val width = min(sw - dp(c, 24), dp(c, 340))
    val lp = overlay(width, WindowManager.LayoutParams.WRAP_CONTENT,
      WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH)
    lp.x = (sw - width) / 2
    // Below the bubble, or above it when it sits low.
    val below = blp.y + b.height + dp(c, 6)
    lp.y = if (below + dp(c, 260) < sh) below else max(dp(c, 24), blp.y - dp(c, 270))
    card.setOnTouchListener { _, e -> if (e.actionMasked == MotionEvent.ACTION_OUTSIDE) closePanel(); false }
    runCatching { wm?.addView(card, lp) }.onFailure { return }
    panel = card
    card.alpha = 0f
    card.translationY = -dp(c, 8).toFloat()
    card.animate().alpha(1f).translationY(0f).setDuration(170).setInterpolator(DecelerateInterpolator()).start()
  }

  /** A face in the card, ringed in their colour while they talk, with a small red mark when they can't speak or hear. */
  @SuppressLint("ViewConstructor")
  private class SeatFace(c: Context, private val p: JSONObject, private val talks: Boolean) : View(c) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val ring = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val base = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#2A2233") }
    private val badge = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F2545B") }
    private val mic = c.getDrawable(if (p.optBoolean("deafened")) R.drawable.ic_b_head else R.drawable.ic_b_mic)!!.mutate()

    override fun onDraw(cv: Canvas) {
      val w = width.toFloat()
      val cx = w / 2
      val r = w * 0.42f
      if (talks) {
        ring.color = colorOf(p); ring.strokeWidth = w * 0.06f
        cv.drawCircle(cx, cx, r + w * 0.045f, ring)
      }
      cv.drawCircle(cx, cx, r, base)
      face(p)?.let { cv.drawBitmap(it, null, RectF(cx - r, cx - r, cx + r, cx + r), paint) }
      if (p.optBoolean("muted") || p.optBoolean("deafened")) {
        val br = w * 0.16f
        val bx = cx + r * 0.72f; val by = cx + r * 0.72f
        cv.drawCircle(bx, by, br, badge)
        val s = (br * 1.3f).toInt()
        mic.setBounds((bx - s / 2).toInt(), (by - s / 2).toInt(), (bx + s / 2).toInt(), (by + s / 2).toInt())
        mic.draw(cv)
      }
    }
  }
}
