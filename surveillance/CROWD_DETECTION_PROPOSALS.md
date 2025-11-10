# Crowd Detection Feature Proposals

**Date**: November 10, 2025  
**Project**: GStreamer × ML Inference Surveillance System  
**Purpose**: Client decision - choose preferred crowd detection approach

---

## 🎯 Version 1: Basic People Counter (Simplest)

### Overview
Simple, lightweight people counting with basic alerts. Minimal UI changes.

### What You Get
- **Real-time person count** displayed on video and TUI
- **Threshold alerts** when count exceeds configurable limit
- **Historical peak tracking** (max count reached)
- **JSON log output** with timestamps and counts

### Use Cases
✅ Retail store occupancy monitoring  
✅ Meeting room capacity compliance  
✅ Simple visitor counting  
✅ COVID-19 capacity restrictions  

### TUI Display
```
┌─ People Monitor ─────────────┐
│ Current: 23 people           │
│ Peak Today: 45 people        │
│ Alert Threshold: 50          │
│ Status: ✅ Normal            │
└──────────────────────────────┘
```

### Video Overlay
```
Top-left corner:
┌──────────────┐
│ 👥 23 PEOPLE │
└──────────────┘
```

### CLI Usage
```bash
# Basic usage
cargo run -r -p gstreamed_ort -- video.mp4 --people-count

# With alert threshold
cargo run -r -p gstreamed_ort -- video.mp4 --people-count --threshold 30

# Output JSON log
cargo run -r -p gstreamed_ort -- video.mp4 --people-count --log-counts counts.json
```

### Output File (counts.json)
```json
{
  "video": "video.mp4",
  "total_frames": 1500,
  "frames": [
    {"frame": 0, "timestamp_ms": 0, "people_count": 12},
    {"frame": 30, "timestamp_ms": 1000, "people_count": 15},
    {"frame": 60, "timestamp_ms": 2000, "people_count": 18}
  ],
  "statistics": {
    "peak_count": 45,
    "peak_frame": 850,
    "average_count": 23.5,
    "threshold_violations": 3
  }
}
```

### Development Time
⏱️ **2-3 days**

### Pros
- ✅ Quick to implement
- ✅ Low computational overhead
- ✅ Easy to understand
- ✅ Clean, minimal UI

### Cons
- ❌ No spatial analysis
- ❌ No crowd behavior tracking
- ❌ Limited insights

---

## 🎯 Version 2: Zone-Based Crowd Analytics (Recommended)

### Overview
Advanced zone-based tracking with entry/exit monitoring, density maps, and crowd flow analysis.

### What You Get
- **Per-zone people counting** (using existing ROI zones)
- **Entry/exit tracking** (directional flow analysis)
- **Crowd density heatmap** (color-coded spatial distribution)
- **Dwell time analysis** (how long people stay in zones)
- **Trend visualization** (sparkline charts in TUI)
- **Smart alerts** (zone-specific thresholds)

### Use Cases
✅ Shopping mall traffic analysis  
✅ Airport security monitoring  
✅ Event venue crowd management  
✅ Queue monitoring  
✅ Building evacuation planning  

### TUI Display
```
┌─ Crowd Analytics ─────────────────────────────┐
│ Total: 45 people │ Trend: ↗️ Rising           │
├────────────────────────────────────────────────┤
│ Zone Breakdown:                                │
│ ┌─ Entrance ─────┐  ┌─ Main Hall ───┐        │
│ │ 12 people      │  │ 28 people      │        │
│ │ Density: Low   │  │ Density: High  │        │
│ │ Avg Stay: 15s  │  │ Avg Stay: 3m   │        │
│ │ ▃▄▅▆▇          │  │ ▂▃▆▇█          │        │
│ └────────────────┘  └────────────────┘        │
│                                                │
│ ┌─ Exit ─────────┐                            │
│ │ 5 people       │                            │
│ │ Density: Low   │  Flow: 23 in / 18 out     │
│ │ Avg Stay: 8s   │  Net Change: +5           │
│ │ ▂▂▃▃▂          │                            │
│ └────────────────┘                            │
├────────────────────────────────────────────────┤
│ Alerts:                                        │
│ ⚠️  Main Hall approaching capacity (28/30)    │
└────────────────────────────────────────────────┘
```

### Video Overlay
```
┌──────────────────────┐
│ 👥 45 PEOPLE         │
│ 📊 Entrance: 12      │
│ 📊 Main: 28 ⚠️       │
│ 📊 Exit: 5           │
└──────────────────────┘

+ Color-coded zone overlays
+ Heatmap showing dense areas
+ Direction arrows for flow
```

### CLI Usage
```bash
# Zone-based analytics
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-analytics --tui

# With density heatmap
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-analytics --heatmap

# Zone configuration from file
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-analytics --zones zones.json

# Custom zone thresholds
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-analytics \
  --zone-threshold entrance=20 \
  --zone-threshold main_hall=30
```

### Output File (crowd_analytics.json)
```json
{
  "video": "video.mp4",
  "total_frames": 1500,
  "zones": {
    "entrance": {
      "frames": [
        {
          "frame": 0,
          "count": 5,
          "density": 0.15,
          "tracked_ids": [1, 3, 7, 12, 15],
          "dwell_times": [12.5, 8.3, 15.2, 9.1, 6.8]
        }
      ],
      "statistics": {
        "peak_count": 18,
        "avg_count": 9.2,
        "avg_dwell_time": 14.5,
        "total_entries": 145,
        "total_exits": 142
      }
    }
  },
  "flow_analysis": {
    "entrance_to_main": 138,
    "main_to_exit": 135,
    "net_occupancy_change": 3
  }
}
```

### Development Time
⏱️ **5-7 days**

### Pros
- ✅ Rich spatial insights
- ✅ Actionable analytics
- ✅ Leverages existing ROI zones
- ✅ Professional monitoring

### Cons
- ❌ Requires zone configuration
- ❌ More complex setup
- ❌ Higher computational cost

---

## 🎯 Version 3: AI Crowd Behavior Analysis (Advanced)

### Overview
ML-powered crowd behavior analysis with anomaly detection, crowd dynamics, and predictive alerts.

### What You Get
- **Everything from Version 2**, plus:
- **Behavior classification** (standing, walking, running, gathering)
- **Anomaly detection** (unusual crowd patterns)
- **Crowd dynamics** (flow velocity, congestion detection)
- **Predictive alerts** (bottleneck prediction, surge forecasting)
- **Social distancing monitoring** (distance violations)
- **Loitering detection** (people staying too long)

### Use Cases
✅ Stadium/concert security  
✅ Public transportation hubs  
✅ Protest monitoring  
✅ Emergency evacuation scenarios  
✅ High-security facilities  

### TUI Display
```
┌─ AI Crowd Intelligence ────────────────────────┐
│ Total: 45 people │ Behavior: Normal ✅         │
├─────────────────────────────────────────────────┤
│ Detected Behaviors:                             │
│  🚶 Walking: 32 │ 🧍 Standing: 10 │ 🏃 Running: 3 │
│                                                 │
│ Crowd Dynamics:                                 │
│  Flow Velocity: 1.2 m/s                        │
│  Congestion: Medium (entrance area)            │
│  Density Gradient: ████▓▓▒▒░░                  │
│                                                 │
│ ⚠️ Alerts:                                      │
│  • Bottleneck forming at entrance              │
│  • Predicted surge in 45s (confidence: 78%)    │
│  • 2 people loitering in restricted area       │
│                                                 │
│ Social Distancing:                              │
│  Violations: 8 pairs (18% of crowd)            │
│  Avg Distance: 1.2m (target: 2m)               │
├─────────────────────────────────────────────────┤
│ Anomalies Detected:                             │
│  🔴 Frame 850: Sudden crowd dispersal          │
│  🟡 Frame 920: Unusual gathering pattern       │
└─────────────────────────────────────────────────┘
```

### Video Overlay
```
- Behavior labels on each person
- Velocity vectors showing movement
- Red circles for social distance violations
- Purple zones for congestion areas
- Orange highlights for loiterers
- Anomaly markers with timestamps
```

### CLI Usage
```bash
# Full AI analysis
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-ai --tui

# Social distancing mode
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-ai --social-distance 2.0

# Loitering detection
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-ai --loiter-threshold 60

# Anomaly detection only
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-ai --anomaly-detection

# Export behavioral analysis
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-ai --export-behaviors
```

### Output File (crowd_ai_analysis.json)
```json
{
  "video": "video.mp4",
  "analysis": {
    "behaviors": {
      "walking": 32,
      "standing": 10,
      "running": 3
    },
    "anomalies": [
      {
        "frame": 850,
        "type": "sudden_dispersal",
        "severity": "high",
        "description": "Rapid crowd movement detected",
        "affected_people": 23
      }
    ],
    "social_distancing": {
      "violations": 8,
      "avg_distance": 1.2,
      "target_distance": 2.0,
      "compliance_rate": 0.82
    },
    "loiterers": [
      {
        "track_id": 42,
        "zone": "restricted_area",
        "duration": 125.5,
        "last_seen_frame": 950
      }
    ],
    "predictions": {
      "bottleneck_risk": 0.78,
      "surge_forecast": {
        "eta_seconds": 45,
        "confidence": 0.78,
        "expected_increase": 15
      }
    }
  }
}
```

### Development Time
⏱️ **15-20 days** (requires additional ML models)

### Pros
- ✅ Cutting-edge insights
- ✅ Proactive monitoring
- ✅ Security-focused
- ✅ Rich behavioral data

### Cons
- ❌ Complex implementation
- ❌ Requires additional ML models
- ❌ Higher computational requirements
- ❌ May need training data

---

## 🎯 Version 4: Quick Stats Dashboard (Minimal TUI)

### Overview
Lightweight statistics overlay - no video changes, TUI-only monitoring for live streams.

### What You Get
- **Live TUI dashboard** (no video overlay at all)
- **Real-time statistics** (current, min, max, average)
- **Time-series graph** (ASCII chart in terminal)
- **CSV export** for analysis in Excel/Python
- **Zero impact on video output**

### Use Cases
✅ Long-duration monitoring  
✅ Headless server deployment  
✅ Multiple camera feeds  
✅ Resource-constrained systems  
✅ Data collection only  

### TUI Display (Full Screen)
```
┌─────────────────────────────────────────────────────┐
│ Crowd Monitoring Dashboard - video.mp4              │
│ Runtime: 00:15:32 | Frame: 27960/30000 | FPS: 30.1  │
├─────────────────────────────────────────────────────┤
│                                                      │
│ 👥 PEOPLE COUNT                                     │
│    Current:  23 people                              │
│    Average:  18.5 people                            │
│    Peak:     45 people (@ 00:12:15)                 │
│    Minimum:  2 people (@ 00:01:03)                  │
│                                                      │
│ 📊 LAST 5 MINUTES                                   │
│                                                      │
│   50 ┤                           ╭──╮               │
│   40 ┤                      ╭────╯  ╰─╮             │
│   30 ┤              ╭───────╯         ╰──╮          │
│   20 ┤      ╭───────╯                    ╰────      │
│   10 ┤  ────╯                                 ───   │
│    0 ┼─────────────────────────────────────────────│
│      0    1m   2m   3m   4m   5m                    │
│                                                      │
│ ⚡ ACTIVITY                                          │
│    Status: Moderate                                 │
│    Change: +5 from last minute                      │
│    Trend:  ↗️ Gradually increasing                  │
│                                                      │
│ 📈 STATISTICS                                       │
│    Total frames processed: 27960                    │
│    People detected: 518,640 instances               │
│    Avg processing time: 12.3ms/frame                │
│                                                      │
│ [S] Screenshot | [R] Reset Stats | [Q] Quit         │
└─────────────────────────────────────────────────────┘
```

### CLI Usage
```bash
# TUI dashboard only (no video overlay)
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-dashboard

# Export to CSV
cargo run -r -p gstreamed_ort -- video.mp4 --crowd-dashboard --export counts.csv

# Live stream monitoring
cargo run -r -p gstreamed_ort -- rtsp://camera/stream --crowd-dashboard --live
```

### Output File (counts.csv)
```csv
timestamp,frame,people_count,processing_time_ms
00:00:00.000,0,12,11.2
00:00:00.033,1,12,12.1
00:00:00.067,2,13,11.8
00:00:00.100,3,13,12.3
```

### Development Time
⏱️ **3-4 days**

### Pros
- ✅ Minimal resource usage
- ✅ No video modification
- ✅ Perfect for headless systems
- ✅ Easy data export
- ✅ Multi-feed friendly

### Cons
- ❌ No visual feedback on video
- ❌ Basic analytics only
- ❌ Limited spatial insights

---

## 📊 Comparison Matrix

| Feature | V1: Basic | V2: Zone-Based | V3: AI Behavior | V4: Dashboard |
|---------|-----------|----------------|-----------------|---------------|
| **People Counting** | ✅ | ✅ | ✅ | ✅ |
| **Video Overlay** | ✅ Basic | ✅ Rich | ✅ Advanced | ❌ None |
| **TUI Dashboard** | ✅ Simple | ✅ Advanced | ✅ AI Insights | ✅ Full Screen |
| **Zone Tracking** | ❌ | ✅ | ✅ | ❌ |
| **Density Heatmap** | ❌ | ✅ | ✅ | ❌ |
| **Flow Analysis** | ❌ | ✅ | ✅ | ❌ |
| **Behavior Detection** | ❌ | ❌ | ✅ | ❌ |
| **Anomaly Detection** | ❌ | ❌ | ✅ | ❌ |
| **Social Distancing** | ❌ | ❌ | ✅ | ❌ |
| **Predictive Alerts** | ❌ | ❌ | ✅ | ❌ |
| **CSV Export** | ❌ | ✅ JSON | ✅ JSON | ✅ CSV |
| **Dev Time** | 2-3 days | 5-7 days | 15-20 days | 3-4 days |
| **CPU Usage** | Low | Medium | High | Low |
| **Complexity** | ⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐ |
| **Best For** | Simple | Analytics | Security | Monitoring |

---

## 💰 Recommended Approach

### Start with: **Version 1 (Basic)** + **Version 4 (Dashboard)**
**Cost**: 5-7 days combined  
**Rationale**: 
- Quick wins to show client
- Low risk, high value
- Can evolve to V2 later
- Covers both visual and data-only use cases

### Then Upgrade to: **Version 2 (Zone-Based)** if client needs:
- Spatial analytics
- Entry/exit tracking
- Multi-zone monitoring

### Consider: **Version 3 (AI)** only if client has:
- Security/safety requirements
- Budget for advanced features
- High-value application (stadiums, airports, etc.)

---

## 🤔 Decision Questions for Client

**Please answer these to help us choose:**

1. **Primary Use Case?**
   - [ ] Simple occupancy counting (V1/V4)
   - [ ] Traffic flow analysis (V2)
   - [ ] Security monitoring (V3)
   - [ ] Data collection only (V4)

2. **Need Video Overlay?**
   - [ ] Yes, must see count on video (V1/V2/V3)
   - [ ] No, statistics only (V4)
   - [ ] Both options

3. **Zone Tracking Required?**
   - [ ] Yes, need per-area counts (V2/V3)
   - [ ] No, total count is enough (V1/V4)

4. **Budget/Timeline?**
   - [ ] Quick delivery (1 week) → V1/V4
   - [ ] Moderate (2 weeks) → V2
   - [ ] Extended (1 month) → V3

5. **Deployment Environment?**
   - [ ] Real-time live feeds (prefer V4)
   - [ ] Recorded video analysis (any version)
   - [ ] Both

6. **Data Export Needed?**
   - [ ] CSV for Excel (V4)
   - [ ] JSON for programming (V2/V3)
   - [ ] Both
   - [ ] Not needed

---

## 📞 Next Steps

**Client, please review and tell us:**
1. Which version interests you most?
2. Any features you'd want to mix/match?
3. Your timeline and budget constraints
4. Specific use case details

We can then create a hybrid version tailored to your exact needs!

---

**Document Created**: November 10, 2025  
**Status**: Awaiting client feedback  
**Contact**: Development Team
