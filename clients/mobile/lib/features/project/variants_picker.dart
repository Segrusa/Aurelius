import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';

/// "Variants": 1, 2 or 3 (up to [max]) with the k× cost note; for New task and starting an idea.
List<Widget> variantsPicker(BuildContext context, {required int max, required int value, required ValueChanged<int> onChanged}) => [
      const Eyebrow('Variants', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (var n = 1; n <= max.clamp(1, 3); n++)
            ChoiceChip(key: Key('variants-$n'), label: Text(n == 1 ? '1 (usual)' : '$n'), selected: value == n, onSelected: (_) => onChanged(n)),
        ],
      ),
      Padding(
        padding: const EdgeInsets.only(top: 6),
        child: Text(variantsCostNote(value), key: const Key('variants-cost'), style: TextStyle(fontSize: 12, color: value > 1 ? Tokens.warning : Theme.of(context).hintColor)),
      ),
    ];
