import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

/// Whether a touch that went down at [down] and up at [up] was a tap and not
/// a scroll or a drag: it moved less than the touch slop.
bool isTap(Offset down, Offset up) => (up - down).distance < kTouchSlop;

/// Closes the keyboard when you tap anywhere outside the focused text field,
/// on every page, sheet and dialog under it. Flutter only does this on the
/// web; on a phone the keyboard otherwise stays until a page closes it.
///
/// Only a tap closes it: a scroll that starts outside the field keeps the
/// keyboard, so you can scroll back through a chat while you type. Buttons
/// that belong to a field (a composer's send button) are kept inside by
/// wrapping them in a [TextFieldTapRegion].
class DismissKeyboardOnTap extends StatefulWidget {
  const DismissKeyboardOnTap({super.key, required this.child});

  final Widget child;

  @override
  State<DismissKeyboardOnTap> createState() => _DismissKeyboardOnTapState();
}

class _DismissKeyboardOnTapState extends State<DismissKeyboardOnTap> {
  // Where the last touch outside the focused field went down.
  Offset? _down;

  @override
  Widget build(BuildContext context) => Actions(
        actions: {
          EditableTextTapOutsideIntent: CallbackAction<EditableTextTapOutsideIntent>(
            onInvoke: (intent) => _down = intent.pointerDownEvent.position,
          ),
          EditableTextTapUpOutsideIntent: CallbackAction<EditableTextTapUpOutsideIntent>(
            onInvoke: (intent) {
              final down = _down;
              _down = null;
              if (down != null && isTap(down, intent.pointerUpEvent.position)) intent.focusNode.unfocus();
              return null;
            },
          ),
        },
        child: widget.child,
      );
}
