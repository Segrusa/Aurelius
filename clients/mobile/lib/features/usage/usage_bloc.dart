import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class UsageEvent {
  const UsageEvent();
}

class UsageRequested extends UsageEvent {
  const UsageRequested({this.refreshLimits = false});

  /// Ask Claude Code for fresh plan limits (otherwise the daemon's cache).
  final bool refreshLimits;
}

/// A day of the 14-day chart was picked: load its hours.
class UsageDaySelected extends UsageEvent {
  const UsageDaySelected(this.dayStartMs);

  final int dayStartMs;
}

class UsageState extends Equatable {
  const UsageState({
    this.loading = true,
    this.usage,
    this.subscription,
    this.error,
    this.selectedDayMs,
    this.hours,
    this.hoursLoading = false,
    this.hoursError,
  });

  final bool loading;
  final UsageSnapshot? usage;
  final SubscriptionUsage? subscription;
  final String? error;

  /// The day the hourly chart shows (a `byDay` start); today by default.
  final int? selectedDayMs;

  /// The selected day's hours, once loaded.
  final UsageHours? hours;
  final bool hoursLoading;
  final String? hoursError;

  UsageState copyWith({
    bool? loading,
    UsageSnapshot? usage,
    SubscriptionUsage? subscription,
    String? Function()? error,
    int? selectedDayMs,
    UsageHours? Function()? hours,
    bool? hoursLoading,
    String? Function()? hoursError,
  }) =>
      UsageState(
        loading: loading ?? this.loading,
        usage: usage ?? this.usage,
        subscription: subscription ?? this.subscription,
        error: error != null ? error() : this.error,
        selectedDayMs: selectedDayMs ?? this.selectedDayMs,
        hours: hours != null ? hours() : this.hours,
        hoursLoading: hoursLoading ?? this.hoursLoading,
        hoursError: hoursError != null ? hoursError() : this.hoursError,
      );

  @override
  List<Object?> get props => [loading, usage, subscription, error, selectedDayMs, hours, hoursLoading, hoursError];
}

class UsageBloc extends Bloc<UsageEvent, UsageState> {
  UsageBloc(this._repository) : super(const UsageState()) {
    on<UsageRequested>(_load, transformer: droppable());
    on<UsageDaySelected>(_loadHours, transformer: restartable());
    add(const UsageRequested());
  }

  final AureliusRepository _repository;

  Future<void> _load(UsageRequested event, Emitter<UsageState> emit) async {
    try {
      final usage = await _repository.usage();
      final subscription = await _repository.subscription(refresh: event.refreshLimits);
      emit(state.copyWith(loading: false, usage: usage, subscription: subscription, error: () => null));
      final days = usage.byDay.map((d) => d.dayStartMs).toList();
      if (days.isNotEmpty) add(UsageDaySelected(days.contains(state.selectedDayMs) ? state.selectedDayMs! : days.last));
    } catch (e) {
      emit(state.copyWith(loading: false, error: () => e.toString()));
    }
  }

  Future<void> _loadHours(UsageDaySelected event, Emitter<UsageState> emit) async {
    final keep = state.hours?.dayStartMs == event.dayStartMs ? state.hours : null;
    emit(state.copyWith(selectedDayMs: event.dayStartMs, hours: () => keep, hoursLoading: true, hoursError: () => null));
    try {
      final hours = await _repository.usageHours(event.dayStartMs);
      emit(state.copyWith(hours: () => hours, hoursLoading: false));
    } catch (e) {
      emit(state.copyWith(hoursLoading: false, hoursError: () => e.toString()));
    }
  }
}

sealed class ContextEvent {
  const ContextEvent();
}

class ContextRequested extends ContextEvent {
  const ContextRequested();
}

class ContextState extends Equatable {
  const ContextState({this.loading = true, this.view, this.error});

  final bool loading;
  final ContextView? view;
  final String? error;

  @override
  List<Object?> get props => [loading, view, error];
}

/// What went into one answer (context inspector).
class ContextBloc extends Bloc<ContextEvent, ContextState> {
  ContextBloc(this._repository, this.runId) : super(const ContextState()) {
    on<ContextRequested>((event, emit) async {
      try {
        emit(ContextState(loading: false, view: await _repository.context(runId)));
      } catch (e) {
        emit(ContextState(loading: false, error: e.toString()));
      }
    });
    add(const ContextRequested());
  }

  final AureliusRepository _repository;
  final String runId;
}
