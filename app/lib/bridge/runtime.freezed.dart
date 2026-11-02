// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'runtime.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$TaskEvent {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'TaskEvent()';
}


}

/// @nodoc
class $TaskEventCopyWith<$Res>  {
$TaskEventCopyWith(TaskEvent _, $Res Function(TaskEvent) __);
}


/// Adds pattern-matching-related methods to [TaskEvent].
extension TaskEventPatterns on TaskEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( TaskEvent_Started value)?  started,TResult Function( TaskEvent_Progress value)?  progress,TResult Function( TaskEvent_Finished value)?  finished,TResult Function( TaskEvent_Cancelled value)?  cancelled,TResult Function( TaskEvent_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case TaskEvent_Started() when started != null:
return started(_that);case TaskEvent_Progress() when progress != null:
return progress(_that);case TaskEvent_Finished() when finished != null:
return finished(_that);case TaskEvent_Cancelled() when cancelled != null:
return cancelled(_that);case TaskEvent_Failed() when failed != null:
return failed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( TaskEvent_Started value)  started,required TResult Function( TaskEvent_Progress value)  progress,required TResult Function( TaskEvent_Finished value)  finished,required TResult Function( TaskEvent_Cancelled value)  cancelled,required TResult Function( TaskEvent_Failed value)  failed,}){
final _that = this;
switch (_that) {
case TaskEvent_Started():
return started(_that);case TaskEvent_Progress():
return progress(_that);case TaskEvent_Finished():
return finished(_that);case TaskEvent_Cancelled():
return cancelled(_that);case TaskEvent_Failed():
return failed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( TaskEvent_Started value)?  started,TResult? Function( TaskEvent_Progress value)?  progress,TResult? Function( TaskEvent_Finished value)?  finished,TResult? Function( TaskEvent_Cancelled value)?  cancelled,TResult? Function( TaskEvent_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case TaskEvent_Started() when started != null:
return started(_that);case TaskEvent_Progress() when progress != null:
return progress(_that);case TaskEvent_Finished() when finished != null:
return finished(_that);case TaskEvent_Cancelled() when cancelled != null:
return cancelled(_that);case TaskEvent_Failed() when failed != null:
return failed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( BigInt taskId)?  started,TResult Function( double fraction)?  progress,TResult Function()?  finished,TResult Function()?  cancelled,TResult Function( ErrorCode code)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case TaskEvent_Started() when started != null:
return started(_that.taskId);case TaskEvent_Progress() when progress != null:
return progress(_that.fraction);case TaskEvent_Finished() when finished != null:
return finished();case TaskEvent_Cancelled() when cancelled != null:
return cancelled();case TaskEvent_Failed() when failed != null:
return failed(_that.code);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( BigInt taskId)  started,required TResult Function( double fraction)  progress,required TResult Function()  finished,required TResult Function()  cancelled,required TResult Function( ErrorCode code)  failed,}) {final _that = this;
switch (_that) {
case TaskEvent_Started():
return started(_that.taskId);case TaskEvent_Progress():
return progress(_that.fraction);case TaskEvent_Finished():
return finished();case TaskEvent_Cancelled():
return cancelled();case TaskEvent_Failed():
return failed(_that.code);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( BigInt taskId)?  started,TResult? Function( double fraction)?  progress,TResult? Function()?  finished,TResult? Function()?  cancelled,TResult? Function( ErrorCode code)?  failed,}) {final _that = this;
switch (_that) {
case TaskEvent_Started() when started != null:
return started(_that.taskId);case TaskEvent_Progress() when progress != null:
return progress(_that.fraction);case TaskEvent_Finished() when finished != null:
return finished();case TaskEvent_Cancelled() when cancelled != null:
return cancelled();case TaskEvent_Failed() when failed != null:
return failed(_that.code);case _:
  return null;

}
}

}

/// @nodoc


class TaskEvent_Started extends TaskEvent {
  const TaskEvent_Started({required this.taskId}): super._();
  

 final  BigInt taskId;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEvent_StartedCopyWith<TaskEvent_Started> get copyWith => _$TaskEvent_StartedCopyWithImpl<TaskEvent_Started>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent_Started&&(identical(other.taskId, taskId) || other.taskId == taskId));
}


@override
int get hashCode {
    return Object.hash(runtimeType,taskId);
}

@override
String toString() {
    return 'TaskEvent.started(taskId: $taskId)';
}


}

/// @nodoc
abstract mixin class $TaskEvent_StartedCopyWith<$Res> implements $TaskEventCopyWith<$Res> {
  factory $TaskEvent_StartedCopyWith(TaskEvent_Started value, $Res Function(TaskEvent_Started) _then) = _$TaskEvent_StartedCopyWithImpl;
@useResult
$Res call({
 BigInt taskId
});




}
/// @nodoc
class _$TaskEvent_StartedCopyWithImpl<$Res>
    implements $TaskEvent_StartedCopyWith<$Res> {
  _$TaskEvent_StartedCopyWithImpl(this._self, this._then);

  final TaskEvent_Started _self;
  final $Res Function(TaskEvent_Started) _then;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? taskId = null,}) {
  return _then(TaskEvent_Started(
taskId: null == taskId ? _self.taskId : taskId // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class TaskEvent_Progress extends TaskEvent {
  const TaskEvent_Progress({required this.fraction}): super._();
  

 final  double fraction;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEvent_ProgressCopyWith<TaskEvent_Progress> get copyWith => _$TaskEvent_ProgressCopyWithImpl<TaskEvent_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent_Progress&&(identical(other.fraction, fraction) || other.fraction == fraction));
}


@override
int get hashCode {
    return Object.hash(runtimeType,fraction);
}

@override
String toString() {
    return 'TaskEvent.progress(fraction: $fraction)';
}


}

/// @nodoc
abstract mixin class $TaskEvent_ProgressCopyWith<$Res> implements $TaskEventCopyWith<$Res> {
  factory $TaskEvent_ProgressCopyWith(TaskEvent_Progress value, $Res Function(TaskEvent_Progress) _then) = _$TaskEvent_ProgressCopyWithImpl;
@useResult
$Res call({
 double fraction
});




}
/// @nodoc
class _$TaskEvent_ProgressCopyWithImpl<$Res>
    implements $TaskEvent_ProgressCopyWith<$Res> {
  _$TaskEvent_ProgressCopyWithImpl(this._self, this._then);

  final TaskEvent_Progress _self;
  final $Res Function(TaskEvent_Progress) _then;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? fraction = null,}) {
  return _then(TaskEvent_Progress(
fraction: null == fraction ? _self.fraction : fraction // ignore: cast_nullable_to_non_nullable
as double,
  ));
}


}

/// @nodoc


class TaskEvent_Finished extends TaskEvent {
  const TaskEvent_Finished(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent_Finished);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'TaskEvent.finished()';
}


}




/// @nodoc


class TaskEvent_Cancelled extends TaskEvent {
  const TaskEvent_Cancelled(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent_Cancelled);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'TaskEvent.cancelled()';
}


}




/// @nodoc


class TaskEvent_Failed extends TaskEvent {
  const TaskEvent_Failed({required this.code}): super._();
  

 final  ErrorCode code;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TaskEvent_FailedCopyWith<TaskEvent_Failed> get copyWith => _$TaskEvent_FailedCopyWithImpl<TaskEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is TaskEvent_Failed&&(identical(other.code, code) || other.code == code));
}


@override
int get hashCode {
    return Object.hash(runtimeType,code);
}

@override
String toString() {
    return 'TaskEvent.failed(code: $code)';
}


}

/// @nodoc
abstract mixin class $TaskEvent_FailedCopyWith<$Res> implements $TaskEventCopyWith<$Res> {
  factory $TaskEvent_FailedCopyWith(TaskEvent_Failed value, $Res Function(TaskEvent_Failed) _then) = _$TaskEvent_FailedCopyWithImpl;
@useResult
$Res call({
 ErrorCode code
});




}
/// @nodoc
class _$TaskEvent_FailedCopyWithImpl<$Res>
    implements $TaskEvent_FailedCopyWith<$Res> {
  _$TaskEvent_FailedCopyWithImpl(this._self, this._then);

  final TaskEvent_Failed _self;
  final $Res Function(TaskEvent_Failed) _then;

/// Create a copy of TaskEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? code = null,}) {
  return _then(TaskEvent_Failed(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as ErrorCode,
  ));
}


}

// dart format on
