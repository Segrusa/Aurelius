import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../account/claude_login.dart' show ownUsage;
import 'usage_bloc.dart';
import '../../core/widgets/ios.dart';

class UsagePage extends StatelessWidget {
  const UsagePage({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) => UsageBloc(context.read<AureliusRepository>()),
      child: Scaffold(
        appBar: GlassAppBar(title: const Text('Usage')),
        body: BlocBuilder<UsageBloc, UsageState>(
          builder: (context, state) {
            if (state.loading) return const Center(child: CircularProgressIndicator());
            final usage = state.usage;
            if (usage == null) return ErrorRetry(state.error ?? 'No usage data.', onRetry: () => context.read<UsageBloc>().add(const UsageRequested()));
            // Only the own login's /usage (B12, R3 m3).
            final sub = ownUsage(state.subscription, context.read<AureliusRepository>().linkState.user?.id);
            return RefreshIndicator(
              onRefresh: () async => context.read<UsageBloc>().add(const UsageRequested(refreshLimits: true)),
              child: ListView(
                padding: EdgeInsets.only(bottom: listBottom(context)),
                children: [
                  if (sub != null && (sub.limits.isNotEmpty || sub.error != null)) ...[
                    Eyebrow('Claude plan${sub.plan != null ? ' · ${sub.plan}' : ''}'),
                    if (sub.error != null) Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: Text(sub.error!, style: TextStyle(color: Tokens.muted))),
                    for (final l in sub.limits) _LimitBar(limit: l),
                  ],
                  const Eyebrow('Tokens'),
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    child: Row(children: [
                      for (final w in usage.windows)
                        Expanded(
                          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                            Text(Fmt.tokens(w.tokens), style: Tokens.title(size: 24)),
                            Text('${w.label} · ${w.runs} runs', style: TextStyle(color: Tokens.muted, fontSize: 12)),
                          ]),
                        ),
                    ]),
                  ),
                  if (usage.byDay.isNotEmpty) ...[
                    const Eyebrow('Last 14 days'),
                    _DayBars(
                      days: usage.byDay,
                      selectedMs: state.selectedDayMs,
                      onSelect: (ms) => context.read<UsageBloc>().add(UsageDaySelected(ms)),
                    ),
                    if (state.selectedDayMs != null) _HourSection(state: state),
                  ],
                  if (usage.bySource.isNotEmpty) ...[
                    const Eyebrow('By connection (30 days)'),
                    for (final r in usage.bySource)
                      ListTile(
                        dense: true,
                        title: Text('${r.connection} · ${r.model}'),
                        subtitle: Text('${r.runs} runs', style: TextStyle(color: Tokens.muted)),
                        trailing: Text(Fmt.tokens(r.inputTokens + r.outputTokens), style: Tokens.monoSmall(color: Tokens.text2)),
                      ),
                  ],
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _LimitBar extends StatelessWidget {
  const _LimitBar({required this.limit});

  final PlanLimitView limit;

  @override
  Widget build(BuildContext context) {
    final color = limit.percent >= 70 ? Tokens.warning : Tokens.accent;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [
          Expanded(child: Text(limit.label)),
          Text('${limit.percent}%', style: Tokens.monoSmall(color: color)),
        ]),
        const SizedBox(height: 6),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: LinearProgressIndicator(value: limit.percent / 100, minHeight: 6, color: color, backgroundColor: Tokens.surface2),
        ),
        if (limit.resets != null) Padding(padding: const EdgeInsets.only(top: 4), child: Text('Resets ${limit.resets}', style: TextStyle(color: Tokens.muted, fontSize: 12))),
      ]),
    );
  }
}

class _DayBars extends StatelessWidget {
  const _DayBars({required this.days, required this.selectedMs, required this.onSelect});

  final List<UsageDay> days;
  final int? selectedMs;
  final ValueChanged<int> onSelect;

  @override
  Widget build(BuildContext context) {
    final top = max(1, days.map((d) => d.tokens).fold(0, max));
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: SizedBox(
        height: 110,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            for (final d in days)
              Expanded(
                child: Tooltip(
                  message: '${_dayLabel(d.dayStartMs)}: ${Fmt.tokens(d.tokens)}',
                  child: GestureDetector(
                    key: ValueKey('usage-day-${d.dayStartMs}'),
                    behavior: HitTestBehavior.opaque,
                    onTap: () => onSelect(d.dayStartMs),
                    // The whole column is the tap target, not only the bar.
                    child: Column(mainAxisAlignment: MainAxisAlignment.end, children: [
                      Container(
                        margin: const EdgeInsets.symmetric(horizontal: 2),
                        height: max(2, 100 * d.tokens / top),
                        decoration: BoxDecoration(
                          color: d.dayStartMs == selectedMs ? Tokens.accent : Tokens.borderStrong,
                          borderRadius: BorderRadius.circular(2),
                        ),
                      ),
                    ]),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

String _dayLabel(int ms) => DateFormat('EEE d MMM').format(DateTime.fromMillisecondsSinceEpoch(ms));

String _cost(int micro, bool estimate) => '${estimate ? '≈ ' : ''}\$${(micro / 1000000).toStringAsFixed(micro < 10000 ? 4 : 2)}';

/// "Per hour" for the selected day: day total, prev/next within the 14 days,
/// and 24 bars.
class _HourSection extends StatelessWidget {
  const _HourSection({required this.state});

  final UsageState state;

  @override
  Widget build(BuildContext context) {
    final days = state.usage!.byDay.map((d) => d.dayStartMs).toList();
    final day = state.selectedDayMs!;
    final at = days.indexOf(day);
    final hours = state.hours?.dayStartMs == day ? state.hours : null;
    final bloc = context.read<UsageBloc>();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Eyebrow('Per hour · ${_dayLabel(day)}'),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4),
        child: Row(children: [
          IconButton(
            key: const ValueKey('usage-prev-day'),
            tooltip: 'Previous day',
            icon: const Icon(Icons.chevron_left),
            onPressed: at > 0 ? () => bloc.add(UsageDaySelected(days[at - 1])) : null,
          ),
          Expanded(
            child: hours == null
                ? const SizedBox.shrink()
                : Column(children: [
                    Text(Fmt.tokens(hours.inputTokens + hours.outputTokens), style: Tokens.title(size: 22)),
                    Text(
                      [
                        '${hours.runs} runs',
                        'in ${Fmt.tokens(hours.inputTokens)}',
                        'out ${Fmt.tokens(hours.outputTokens)}',
                        if (hours.costMicroUsd != null) _cost(hours.costMicroUsd!, hours.estimate),
                      ].join(' · '),
                      style: TextStyle(color: Tokens.muted, fontSize: 12),
                    ),
                  ]),
          ),
          IconButton(
            key: const ValueKey('usage-next-day'),
            tooltip: 'Next day',
            icon: const Icon(Icons.chevron_right),
            onPressed: at >= 0 && at < days.length - 1 ? () => bloc.add(UsageDaySelected(days[at + 1])) : null,
          ),
        ]),
      ),
      if (state.hoursError != null)
        Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 0), child: Text(state.hoursError!, style: TextStyle(color: Tokens.muted)))
      else if (hours == null)
        const Padding(padding: EdgeInsets.all(24), child: Center(child: CircularProgressIndicator()))
      else
        _HourBars(key: ValueKey('usage-hours-$day'), hours: hours.hours),
    ]);
  }
}

class _HourBars extends StatefulWidget {
  const _HourBars({super.key, required this.hours});

  final List<UsageHour> hours;

  @override
  State<_HourBars> createState() => _HourBarsState();
}

class _HourBarsState extends State<_HourBars> {
  int? _selected;

  @override
  Widget build(BuildContext context) {
    final hours = widget.hours;
    final top = max(1, hours.map((h) => h.tokens).fold(0, max));
    final input = Tokens.accent.withValues(alpha: 0.4);
    final picked = _selected != null && _selected! < hours.length ? hours[_selected!] : null;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        SizedBox(
          height: 110,
          child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
            for (var i = 0; i < hours.length; i++)
              Expanded(
                child: GestureDetector(
                  key: ValueKey('usage-hour-$i'),
                  behavior: HitTestBehavior.opaque,
                  onTap: () => setState(() => _selected = _selected == i ? null : i),
                  child: Opacity(
                    opacity: _selected == null || _selected == i ? 1 : 0.45,
                    child: Column(mainAxisAlignment: MainAxisAlignment.end, children: [
                      // Output stacked on input; an empty hour keeps a 1 px stub.
                      if (hours[i].outputTokens > 0)
                        Container(margin: const EdgeInsets.symmetric(horizontal: 1), height: 100 * hours[i].outputTokens / top, color: Tokens.accent),
                      Container(
                        margin: const EdgeInsets.symmetric(horizontal: 1),
                        height: max(1, 100 * hours[i].inputTokens / top),
                        color: hours[i].inputTokens > 0 ? input : Tokens.borderStrong,
                      ),
                    ]),
                  ),
                ),
              ),
          ]),
        ),
        const SizedBox(height: 4),
        Row(children: [
          for (final h in const [0, 6, 12, 18]) Expanded(child: Text(h.toString().padLeft(2, '0'), style: Tokens.monoSmall(color: Tokens.muted))),
        ]),
        const SizedBox(height: 8),
        Row(children: [
          _Swatch(color: input),
          const SizedBox(width: 4),
          Text('Input', style: TextStyle(color: Tokens.muted, fontSize: 12)),
          const SizedBox(width: 12),
          _Swatch(color: Tokens.accent),
          const SizedBox(width: 4),
          Text('Output', style: TextStyle(color: Tokens.muted, fontSize: 12)),
        ]),
        if (picked != null) ...[
          const SizedBox(height: 10),
          _HourDetail(index: _selected!, hour: picked),
        ],
      ]),
    );
  }
}

class _Swatch extends StatelessWidget {
  const _Swatch({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) => Container(width: 10, height: 10, decoration: BoxDecoration(color: color, borderRadius: BorderRadius.circular(2)));
}

class _HourDetail extends StatelessWidget {
  const _HourDetail({required this.index, required this.hour});

  final int index;
  final UsageHour hour;

  @override
  Widget build(BuildContext context) {
    final from = index.toString().padLeft(2, '0');
    final to = ((index + 1) % 24).toString().padLeft(2, '0');
    final rows = <(String, String)>[
      ('Input', Fmt.tokens(hour.inputTokens)),
      ('Output', Fmt.tokens(hour.outputTokens)),
      ('Cached', Fmt.tokens(hour.cachedTokens)),
      ('Runs', '${hour.runs}'),
      if (hour.costMicroUsd != null) ('Cost', _cost(hour.costMicroUsd!, hour.estimate)),
    ];
    return Container(
      key: const ValueKey('usage-hour-detail'),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(color: Tokens.surface2, borderRadius: BorderRadius.circular(10)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('$from:00–$to:00', style: const TextStyle(fontWeight: FontWeight.w600)),
        const SizedBox(height: 6),
        for (final (label, value) in rows)
          Row(children: [
            Expanded(child: Text(label, style: TextStyle(color: Tokens.muted, fontSize: 13))),
            Text(value, style: Tokens.monoSmall(color: Tokens.text2)),
          ]),
      ]),
    );
  }
}
