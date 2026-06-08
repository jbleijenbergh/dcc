# Wacom Tablet Support

## Overview

The DCC Painter application now supports pressure-sensitive input from Wacom tablets and other digitizers through the `WindowEvent::Touch` API provided by winit 0.30.

## Features Implemented

### ✅ Pressure Sensitivity
- **Brush Size**: Dynamically scales with pressure (20% to 100% of base size)
- **Brush Opacity**: Directly multiplied by pressure for natural stroke variation
- **Real-time Feedback**: Pressure value displayed in the UI when tablet is active

### ✅ Touch Event Handling
- Properly handles `TouchPhase::Started`, `TouchPhase::Moved`, and `TouchPhase::Ended`
- Seamlessly integrates with existing painting workflow
- Stroke recording with pressure-aware parameters

### ✅ UI Indicators
- Green "📱 Tablet Input Active" indicator when tablet is detected
- Live pressure bar showing current pressure value (0.0 to 1.0)
- Displayed in the "Brush Settings" panel

## How It Works

### Event Flow
1. When you use a Wacom tablet, the system sends `WindowEvent::Touch` events
2. Input normalization extracts pressure from `touch.force` (either `Normalized` or `Calibrated` force)
3. ECS `AppEvent` input/tool commands are emitted and processed in the frame update path
4. Pressure is applied to brush size and opacity in `paint_at_cursor()`
5. Strokes are recorded with pressure-sensitive parameters

### Code Changes

**State Fields Added:**
```rust
pen_pressure: f32,       // 0.0 to 1.0 (from touch force)
has_tablet_input: bool,  // Track if we're receiving tablet events
```

**Input Normalization (`src/app/input/mod.rs`):**
- Handles `WindowEvent::Touch` normalization
- Extracts pressure from `Force::Normalized` or `Force::Calibrated`
- Emits ECS input/tool events for touch phases (Started/Moved/Ended/Cancelled)
- Updates pointer position and modifier snapshots consistently with other input sources

**Runtime Event Processing (`src/app/mod.rs`):**
- Drains normalized ECS events during frame update
- Applies input/tool/domain updates in ECS-native event execution path

**Paint Function (src/app/actions.rs):**
- Applies pressure scaling to brush size: `effective_size = base_size * (0.2 + 0.8 * pressure)`
- Applies pressure to opacity: `effective_opacity = base_opacity * pressure`
- Logs pressure value with stroke start coordinates

## Platform Support

### macOS
- ✅ Pressure sensitivity fully supported
- Works with Wacom tablets, Apple Pencil (on iPad Sidecar), and trackpad force touch

### Windows
- ✅ Should work with Windows 8+ tablet APIs
- Supports Wacom and other Windows Ink devices

### Linux
- ⚠️  Touch events supported but may vary by window manager
- Best results with Wayland

## Limitations & Future Enhancements

### Current Limitations
❌ **Tilt/Rotation**: winit 0.30's Touch API doesn't expose pen tilt or rotation angles  
❌ **Eraser Detection**: No automatic tool switching when pen is flipped  
❌ **Barrel Buttons**: Pen button events not available through Touch API

### Possible Future Enhancements
1. **Platform-Specific Extensions**
   - Use `winit::platform::macos` for NSEvent tablet data (tilt, rotation, eraser)
   - Use `winit::platform::windows` for Windows Ink data
   - Requires conditional compilation and platform-specific code

2. **Third-Party Libraries**
   - Consider `evdev` (Linux), `hidapi`, or `tablet-rs` for full tablet support
   - Would add dependencies but provide complete Wacom feature set

3. **Brush Dynamics**
   - Add tilt-based brush shape deformation
   - Rotation-aware brush tips
   - Velocity-based size/opacity variations

## Testing

### Without a Tablet
The app continues to work normally with a mouse. `has_tablet_input` remains `false` and pressure defaults to 1.0.

### With a Tablet
1. Launch the app: `cargo run`
2. Use your Wacom pen or tablet stylus to paint
3. Vary pressure to see size and opacity changes
4. Check the "Brush Settings" panel to see the pressure indicator

### Debug Logging
Enable debug logging to see pressure values:
```bash
RUST_LOG=debug cargo run
```

Look for log lines like:
```
Touch input: pressure=0.543, phase=Moved
Stroke start coordinates:
  ...
  Pressure: 0.543, Size: 19.3
```

## Technical Details

### Pressure Mapping Formula
```rust
// Minimum 20% size even at zero pressure
let effective_size = base_size * (0.2 + 0.8 * pressure);

// Opacity directly scaled
let effective_opacity = base_opacity * pressure;
```

This ensures brushes remain visible even with light pressure, while still providing a 5x dynamic range.

### Force Data Handling
```rust
self.pen_pressure = match touch.force {
    Some(Force::Normalized(pressure)) => pressure as f32,
    Some(Force::Calibrated { force, max_possible_force, .. }) => {
        (force / max_possible_force) as f32
    }
    None => 1.0,
};
```

The normalized force is preferred when available. Calibrated force is converted to a 0-1 range. If no force data is available, defaults to full pressure.

## Contributing

If you have a Wacom tablet and encounter issues:
1. Check debug logs for Touch events
2. Report your OS, tablet model, and winit version
3. Include pressure values if they seem incorrect

For advanced tablet features (tilt/rotation), contributions welcome using platform-specific APIs!
