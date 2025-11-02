# ROI Zone Feature - Implementation Progress

**Project**: GStreamer ML Inference with Ratatui TUI  
**Branch**: `feature/roi-zones`  
**Started**: November 2, 2025  
**Status**: Phase 1 Complete ✅

---

## 📋 Implementation Phases

### **Phase 1: Core Foundation** ✅ COMPLETE
**Duration**: ~2 hours  
**Goal**: Data structures + basic zone logic

#### Completed Tasks:
- ✅ Created `gstreamed_ort/src/tui/roi.rs` module (293 lines)
- ✅ Implemented `RoiZone` struct with ID, name, bbox, enabled flag
- ✅ Implemented `RoiBBox` struct with normalized coordinates (0.0-1.0)
- ✅ Added `contains_detection()` method using center-point algorithm
- ✅ Added `validate_and_clamp()` for bbox validation
- ✅ Implemented JSON persistence (`save_zones()`, `load_zones()`)
- ✅ Extended `App` struct with ROI fields:
  - `tui_mode: TuiMode` (Monitor/ZoneList/ZoneEdit)
  - `zones: Vec<RoiZone>`
  - `selected_zone_idx: usize`
  - `zone_draft: Option<RoiZone>`
- ✅ Added zone management methods to `App`:
  - `get_zone_detections()`
  - `create_zone()`
  - `delete_zone()`
  - `toggle_zone()`
  - `save_zones()`
  - `count_zone_detections()`
- ✅ Added dependencies: `uuid`, `serde`
- ✅ Zones auto-load on App startup
- ✅ Created 9 comprehensive unit tests (all passing):
  - `test_zone_creation`
  - `test_bbox_area`
  - `test_contains_detection_center_point`
  - `test_zone_boundary_cases`
  - `test_disabled_zone`
  - `test_validate_and_clamp`
  - `test_save_load_roundtrip`
  - `test_load_nonexistent_file`
  - `test_bbox_clamping`

#### Files Modified/Created:
```
surveillance/Cargo.toml              (+1 line - uuid dependency)
gstreamed_ort/Cargo.toml            (+2 lines - uuid, serde)
gstreamed_ort/src/tui/mod.rs        (+1 line - roi module)
gstreamed_ort/src/tui/app.rs        (+65 lines - ROI state & methods)
gstreamed_ort/src/tui/roi.rs        (NEW - 293 lines)
gstreamed_ort/test_roi_phase1.rs    (NEW - test program)
```

#### Test Results:
```
running 9 tests
test tui::roi::tests::test_bbox_area ... ok
test tui::roi::tests::test_bbox_clamping ... ok
test tui::roi::tests::test_contains_detection_center_point ... ok
test tui::roi::tests::test_disabled_zone ... ok
test tui::roi::tests::test_load_nonexistent_file ... ok
test tui::roi::tests::test_save_load_roundtrip ... ok
test tui::roi::tests::test_validate_and_clamp ... ok
test tui::roi::tests::test_zone_boundary_cases ... ok
test tui::roi::tests::test_zone_creation ... ok

test result: ok. 9 passed; 0 failed; 0 ignored
```

#### Deliverables Achieved:
- ✅ Can create `RoiZone` programmatically
- ✅ Save zones to `zones.json`
- ✅ Load zones from `zones.json`
- ✅ Test `contains_detection()` with mock data
- ✅ Compile without errors
- ✅ All unit tests pass

---

### **Phase 2: TUI Interface** 🔄 IN PROGRESS
**Estimated Duration**: 3-4 days  
**Goal**: Interactive zone creation and management

#### Planned Tasks:
- [ ] Create zone list view in `ui.rs`
  - [ ] Table showing all zones
  - [ ] Show zone name, status, detection count
  - [ ] Highlight selected zone
- [ ] Create zone editor form in `ui.rs`
  - [ ] Split layout: form (left) + preview (right)
  - [ ] Input fields for name and coordinates
  - [ ] ASCII preview of zone rectangle
- [ ] Add keyboard handlers in `mod.rs`
  - [ ] Monitor mode: 'Z' to enter zone management
  - [ ] Zone list mode: navigation, create, edit, delete, toggle
  - [ ] Zone edit mode: field navigation, coordinate adjustment
- [ ] Implement mode switching logic
- [ ] Add visual feedback for zone operations

#### Files to Modify:
```
gstreamed_ort/src/tui/ui.rs         (add ~150 lines)
gstreamed_ort/src/tui/mod.rs        (add ~50 lines)
```

---

### **Phase 3: Detection Integration** ⏳ PENDING
**Estimated Duration**: 1-2 days  
**Goal**: Filter detections by zones, show in UI

#### Planned Tasks:
- [ ] Implement detection filtering in app update loop
- [ ] Add zone column to detection table
- [ ] Add zone statistics to zone list
- [ ] Highlight detections in active zones
- [ ] Update zone detection counts in real-time
- [ ] Test with real video processing

#### Files to Modify:
```
gstreamed_ort/src/tui/ui.rs         (modify detection table)
gstreamed_ort/src/tui/app.rs        (add filtering logic)
```

---

## 🎯 Design Decisions

### Rectangle-Only Initially
**Rationale**: 90% of surveillance zones are rectangular. Polygons add 3x complexity for 10% benefit.

**Implementation**:
```rust
pub struct RoiBBox {
    pub xmin: f32,  // 0.0 = left edge, 1.0 = right edge
    pub ymin: f32,  // 0.0 = top, 1.0 = bottom
    pub xmax: f32,
    pub ymax: f32,
}
```

### Normalized Coordinates (0.0-1.0)
**Rationale**: Resolution-independent. Works with any video size.

**Example**:
- Zone: `xmin=0.25, ymin=0.25, xmax=0.75, ymax=0.75`
- On 1920x1080: (480, 270) → (1440, 810)
- On 640x480: (160, 120) → (480, 360)

### Center-Point Detection Method
**Rationale**: Simple, fast, and works for most use cases.

**Logic**: Detection is "in zone" if its center point falls within the zone bbox.

---

## 📊 Statistics

### Code Added (Phase 1):
- **Total Lines**: ~360 lines
- **Production Code**: 293 lines (roi.rs)
- **App Integration**: 67 lines (app.rs, mod.rs)
- **Test Code**: 171 lines (9 unit tests)

### Dependencies Added:
- `uuid = "1.11.0"` (with v4 feature)
- `serde = "1.0"` (with derive feature)

---

## 🧪 Testing Strategy

### Unit Tests (Phase 1):
- ✅ Zone creation and initialization
- ✅ BBox area calculation
- ✅ Detection containment logic
- ✅ Boundary edge cases
- ✅ Disabled zone behavior
- ✅ Validation and clamping
- ✅ Save/load persistence
- ✅ Error handling

### Manual Tests (Upcoming):
- [ ] Create zones via TUI
- [ ] Edit zone coordinates
- [ ] Delete zones
- [ ] Toggle zone enable/disable
- [ ] Verify persistence across restarts
- [ ] Test with live video feed

---

## 🚀 Next Steps

### Immediate (Phase 2 - Day 1):
1. Implement `draw_zone_list()` widget
2. Add keyboard handler for 'Z' key in monitor mode
3. Implement basic zone list navigation (up/down)

### Short Term (Phase 2 - Days 2-3):
1. Implement `draw_zone_editor()` widget
2. Add form field navigation (Tab)
3. Add coordinate adjustment (arrow keys)

### Medium Term (Phase 2 - Day 4):
1. Polish UI/UX
2. Add error messages
3. Test zone creation flow end-to-end

---

## 🎓 Key Learnings

### What Went Well:
- Normalized coordinates make zones resolution-independent
- Center-point detection is simple and effective
- Unit tests caught several edge cases early
- JSON persistence is trivial with serde

### Challenges:
- Had to update test helper to match actual `DetectionLog` structure
- Needed to check field names (`frame_number` vs `frame_num`, etc.)

### Code Quality Notes:
- All tests pass ✅
- No clippy warnings (for roi.rs)
- Follows existing code patterns
- Good separation of concerns

---

## 📝 Notes

- Zones are stored in `zones.json` at project root
- Zone IDs are auto-generated with format `zone_<8-char-uuid>`
- Default zone size is 50% of frame (0.25→0.75 on both axes)
- Disabled zones don't filter detections
- App loads zones on startup with graceful error handling

---

**Last Updated**: November 2, 2025  
**Next Review**: After Phase 2 completion
