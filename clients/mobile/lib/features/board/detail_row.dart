import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';

/// A label and its value on one line; the value is cut, never the row.
/// Tappable rows have a chevron, locked ones a lock (§8).
class DetailRow extends StatelessWidget {
  const DetailRow({super.key, required this.label, required this.value, this.onTap, this.color, this.locked = false});

  final String label;
  final String value;
  final VoidCallback? onTap;
  final Color? color;
  final bool locked;

  @override
  Widget build(BuildContext context) => InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          child: Row(children: [
            Text(label, style: TextStyle(fontSize: 17, color: Tokens.label)),
            const SizedBox(width: 12),
            Expanded(child: Text(value, textAlign: TextAlign.end, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 17, color: color ?? Tokens.label2))),
            if (locked) Padding(padding: const EdgeInsets.only(left: 6), child: Icon(CupertinoIcons.lock_fill, size: 16, color: Tokens.label2, semanticLabel: 'Locked')),
            if (onTap != null && !locked) const Padding(padding: EdgeInsets.only(left: 6), child: Chevron()),
          ]),
        ),
      );
}

