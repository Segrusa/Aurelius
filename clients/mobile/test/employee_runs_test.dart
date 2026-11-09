import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/employee_runs/employee_runs_page.dart';
import 'package:flutter_test/flutter_test.dart';

EmployeeRunLine line(Map<String, dynamic> overrides) => EmployeeRunLine.fromJson({
      'employee_id': '01a1a741-0000-7000-8000-000000000000',
      'name': 'Forge',
      'status': 'active',
      'model': 'claude-opus-5-5',
      'model_source': 'company',
      'effort': null,
      'effort_source': 'runtime',
      'model_default_effort': null,
      'supported_efforts': ['low', 'high'],
      'own': <String, dynamic>{},
      ...overrides,
    });

void main() {
  test('a chosen effort says where it comes from', () {
    expect(employeeRunLine(line({'effort': 'high', 'effort_source': 'employee'})), 'Opus 5.5 · High (chosen for this employee)');
    expect(employeeRunLine(line({'effort': 'low', 'effort_source': 'company'})), 'Opus 5.5 · Low (company default)');
  });

  test('without a choice it shows the model default', () {
    expect(employeeRunLine(line({'model_default_effort': 'medium'})), "Opus 5.5 · Medium (model's default)");
    expect(employeeRunLine(line({})), "Opus 5.5 · Model's default");
  });

  test('a model without effort says so', () {
    expect(employeeRunLine(line({'supported_efforts': <String>[]})), 'Opus 5.5 · No effort setting');
  });
}
