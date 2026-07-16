package com.chainchess.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  // 启用返回键拦截，使 WebView 能通过 hash 导航回退
  override val handleBackNavigation: Boolean = true
}
