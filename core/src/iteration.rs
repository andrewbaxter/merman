//! Merman's idle iteration queue: prioritized tasks run in batches from a
//! timer the host schedules, plus the lay-bricks and hover tasks that live on
//! the context.
use crate::context::{AlignId, BrickId, Context, CourseId, TaskId, Vector, VisualId};
use crate::visual::ExtendBrickResult;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Task priorities (merman `IterationTask.P`); higher runs first.
pub struct P;

impl P {
    pub const HOVER: f64 = 500.;
    pub const COURSE_PLACE: f64 = 170.;
    pub const COURSE_COMPACT: f64 = 165.;
    pub const WALL_ADJUST: f64 = 160.;
    pub const LAY_BRICKS: f64 = 150.;
    pub const WALL_COMPACT: f64 = 110.;
    pub const COURSE_EXPAND: f64 = -95.;
    pub const WALL_EXPAND: f64 = -100.;
}

pub enum TaskKind {
    CoursePlace {
        course: CourseId,
        changed: HashSet<BrickId>,
        remove_max_ascent: f64,
        remove_max_descent: f64,
    },
    CourseCompact {
        course: CourseId,
        skip: HashSet<VisualId>,
        skip_primitives: HashSet<VisualId>,
    },
    CourseExpand {
        course: CourseId,
    },
    WallAdjust {
        forward: usize,
        backward: isize,
    },
    WallCompact {
        at: usize,
        /// The course compact sub-task's skip sets while one is in progress.
        compact: Option<(HashSet<VisualId>, HashSet<VisualId>)>,
    },
    WallExpand {
        at: usize,
        expanding: Option<CourseId>,
    },
    LayBricks {
        ends: Vec<BrickId>,
        starts: Vec<BrickId>,
    },
    Hover {
        point: Option<Vector>,
        at: Option<BrickId>,
    },
    ConcensusAlign {
        align: AlignId,
    },
}

pub struct Task {
    pub kind: TaskKind,
    pub priority: f64,
    pub destroyed: bool,
}

/// State shared by the tasks of one flush (merman `IterationContext`).
#[derive(Default)]
pub struct IterationContext {
    pub expanded: HashSet<VisualId>,
}

pub struct QueueEntry {
    pub priority: f64,
    pub seq: u64,
    pub task: TaskId,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        return self.seq == other.seq;
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        return Some(self.cmp(other));
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then earlier queued first
        return self
            .priority
            .partial_cmp(&other.priority)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.seq.cmp(&self.seq));
    }
}

impl Context {
    pub fn task_kind_mut(&mut self, task: TaskId) -> Option<&mut TaskKind> {
        return self.tasks[task].as_mut().map(|t| &mut t.kind);
    }

    /// Queue a task (merman `Context.addIteration`); asks the host for the
    /// idle timer if none is pending.
    pub fn add_iteration(&mut self, kind: TaskKind, priority: f64) -> TaskId {
        let id = self.tasks.len();
        self.tasks.push(Some(Task {
            kind,
            priority,
            destroyed: false,
        }));
        self.enqueue(id, priority);
        return id;
    }

    fn enqueue(&mut self, id: TaskId, priority: f64) {
        self.task_seq += 1;
        self.queue.push(QueueEntry {
            priority,
            seq: self.task_seq,
            task: id,
        });
        if !self.iteration_timer {
            self.iteration_timer = true;
            self.timer_requested = true;
        }
    }

    /// The host asks whether to schedule the idle timer; true at most once per
    /// pending timer.
    pub fn take_timer_request(&mut self) -> bool {
        return std::mem::take(&mut self.timer_requested);
    }

    pub fn task_destroy(&mut self, task: TaskId) {
        let Some(t) = self.tasks[task].as_mut() else {
            return;
        };
        assert!(!t.destroyed, "task destroyed twice");
        t.destroyed = true;
        let kind = std::mem::replace(
            &mut t.kind,
            TaskKind::WallAdjust {
                forward: 0,
                backward: 0,
            },
        );
        self.tasks[task] = None;
        match kind {
            TaskKind::CoursePlace { course, .. } => {
                if self.courses[course].idle_place == Some(task) {
                    self.courses[course].idle_place = None;
                }
            }
            TaskKind::CourseCompact { course, .. } => {
                if self.courses[course].idle_compact == Some(task) {
                    self.courses[course].idle_compact = None;
                }
            }
            TaskKind::CourseExpand { course } => {
                if self.courses[course].idle_expand == Some(task) {
                    self.courses[course].idle_expand = None;
                }
            }
            TaskKind::WallAdjust { .. } => {
                if self.wall.idle_adjust == Some(task) {
                    self.wall.idle_adjust = None;
                }
            }
            TaskKind::WallCompact { .. } => {
                if self.wall.idle_compact == Some(task) {
                    self.wall.idle_compact = None;
                }
            }
            TaskKind::WallExpand { .. } => {
                if self.wall.idle_expand == Some(task) {
                    self.wall.idle_expand = None;
                }
            }
            TaskKind::LayBricks { .. } => {
                if self.idle_lay_bricks == Some(task) {
                    self.idle_lay_bricks = None;
                }
            }
            TaskKind::Hover { .. } => {
                if self.hover_idle == Some(task) {
                    self.hover_idle = None;
                }
            }
            TaskKind::ConcensusAlign { align } => self.concensus_align_destroyed(align),
        }
    }

    /// Merman `handleTimer`: the idle timer fired.
    pub fn handle_timer(&mut self, now_ms: &mut dyn FnMut() -> f64) {
        if self.iteration_pending {
            return;
        }
        self.iteration_pending = true;
        self.flush_iteration(1000, now_ms);
        self.iteration_pending = false;
        self.iteration_timer = false;
        // Anything still queued needs another timer (merman destroyed leftover
        // tasks here, which would drop pending placement)
        if !self.queue.is_empty() {
            self.iteration_timer = true;
            self.timer_requested = true;
        }
    }

    /// Run up to `limit` tasks within 500ms (merman `flushIteration`).
    pub fn flush_iteration(&mut self, limit: usize, now_ms: &mut dyn FnMut() -> f64) {
        let start = now_ms();
        let mut iteration = IterationContext::default();
        for i in 0..limit {
            if i % 100 == 0 && now_ms() > start + 500. {
                break;
            }
            let Some(entry) = self.queue.pop() else {
                break;
            };
            let Some(task) = self.tasks[entry.task].as_ref() else {
                continue;
            };
            if task.destroyed {
                continue;
            }
            let again = self.run_task(entry.task, &mut iteration);
            if again {
                self.enqueue(entry.task, entry.priority);
            } else {
                self.task_destroy(entry.task);
            }
        }
    }

    /// Whether any task is queued.
    pub fn iteration_idle(&self) -> bool {
        return self.queue.is_empty();
    }

    fn run_task(&mut self, task: TaskId, iteration: &mut IterationContext) -> bool {
        let kind = &self.tasks[task].as_ref().unwrap().kind;
        match kind {
            TaskKind::CoursePlace { course, .. } => {
                let c = *course;
                return self.run_course_place(task, c);
            }
            TaskKind::CourseCompact { course, .. } => {
                let c = *course;
                return self.run_course_compact(task, c);
            }
            TaskKind::CourseExpand { course } => {
                let c = *course;
                return self.course_expand_step(c, iteration);
            }
            TaskKind::WallAdjust { .. } => return self.run_wall_adjust(task),
            TaskKind::WallCompact { .. } => return self.run_wall_compact(task),
            TaskKind::WallExpand { .. } => return self.run_wall_expand(task, iteration),
            TaskKind::LayBricks { .. } => return self.run_lay_bricks(task),
            TaskKind::Hover { .. } => return self.run_hover(task),
            TaskKind::ConcensusAlign { align } => {
                let a = *align;
                return self.run_concensus_align(a);
            }
        }
    }

    // ---- Lay bricks --------------------------------------------------------

    fn ensure_idle_lay_bricks(&mut self) -> TaskId {
        if let Some(t) = self.idle_lay_bricks {
            return t;
        }
        let t = self.add_iteration(
            TaskKind::LayBricks {
                ends: vec![],
                starts: vec![],
            },
            P::LAY_BRICKS,
        );
        self.idle_lay_bricks = Some(t);
        return t;
    }

    pub fn trigger_idle_lay_bricks_after_end(&mut self, end: BrickId) {
        let t = self.ensure_idle_lay_bricks();
        if let Some(TaskKind::LayBricks { ends, .. }) = self.task_kind_mut(t) {
            if !ends.contains(&end) {
                ends.push(end);
            }
        }
    }

    pub fn trigger_idle_lay_bricks_before_start(&mut self, start: BrickId) {
        let t = self.ensure_idle_lay_bricks();
        if let Some(TaskKind::LayBricks { starts, .. }) = self.task_kind_mut(t) {
            if !starts.contains(&start) {
                starts.push(start);
            }
        }
    }

    pub fn trigger_idle_lay_bricks_outward(&mut self) {
        let Some(first_course) = self.wall.children.first().copied() else {
            return;
        };
        let last_course = *self.wall.children.last().unwrap();
        let first = self.courses[first_course].children[0];
        let last = *self.courses[last_course].children.last().unwrap();
        self.trigger_idle_lay_bricks_before_start(first);
        self.trigger_idle_lay_bricks_after_end(last);
    }

    /// Merman `Context.triggerIdleLayBricks` for lines added to a primitive.
    pub fn trigger_idle_lay_bricks_lines(&mut self, primitive: VisualId, index: usize, add_count: usize) {
        let size = self.visual_primitive(primitive).lines.len();
        let line_brick = |ctx: &Context, i: usize| ctx.visual_primitive(primitive).lines[i].brick;
        if index > 0 {
            if let Some(previous) = line_brick(self, index - 1) {
                self.trigger_idle_lay_bricks_after_end(previous);
                return;
            }
            if index + add_count < size {
                // Hits neither edge
                let Some(next) = line_brick(self, index + add_count) else {
                    return;
                };
                self.trigger_idle_lay_bricks_before_start(next);
            } else {
                // Hits end edge
                let Some(next) = self.parent_get_next_brick(primitive) else {
                    return;
                };
                self.trigger_idle_lay_bricks_before_start(next);
            }
        } else if index + add_count < size {
            // Hits index edge
            if let Some(next) = line_brick(self, index + add_count) {
                self.trigger_idle_lay_bricks_before_start(next);
                return;
            }
            let Some(previous) = self.parent_get_previous_brick(primitive) else {
                return;
            };
            self.trigger_idle_lay_bricks_after_end(previous);
        } else {
            // Hits both edges
            if let Some(previous) = self.parent_get_previous_brick(primitive) {
                self.trigger_idle_lay_bricks_after_end(previous);
                return;
            }
            let Some(next) = self.parent_get_next_brick(primitive) else {
                return;
            };
            self.trigger_idle_lay_bricks_before_start(next);
        }
    }

    /// Merman `IterationLayBricks`: extend from known bricks a batch at a time.
    fn run_lay_bricks(&mut self, task: TaskId) -> bool {
        for _ in 0..self.config.lay_brick_batch_size {
            let (end, start) = match self.task_kind_mut(task) {
                Some(TaskKind::LayBricks { ends, starts }) => {
                    if ends.is_empty() && starts.is_empty() {
                        return false;
                    }
                    (ends.pop(), starts.pop())
                }
                _ => unreachable!(),
            };
            if let Some(end) = end {
                if self.bricks[end].alive && self.bricks[end].course.is_some() {
                    let inter = self.bricks[end].inter;
                    if let ExtendBrickResult::Brick(created) = self.brick_create_next(inter) {
                        self.brick_add_after(end, created);
                        if let Some(TaskKind::LayBricks { ends, .. }) = self.task_kind_mut(task) {
                            ends.push(created);
                        }
                    }
                }
            }
            if let Some(start) = start {
                if self.bricks[start].alive && self.bricks[start].course.is_some() {
                    let inter = self.bricks[start].inter;
                    if let ExtendBrickResult::Brick(created) = self.brick_create_previous(inter) {
                        self.brick_add_before(start, created);
                        if let Some(TaskKind::LayBricks { starts, .. }) = self.task_kind_mut(task) {
                            starts.push(created);
                        }
                    }
                }
            }
        }
        return true;
    }

    // ---- Hover -------------------------------------------------------------

    /// Merman `HoverIteration.inner`: find the brick under the point, walking
    /// from the last hovered brick. Returns (more to do, hover changed).
    fn hover_inner(&mut self, task: TaskId) -> (bool, bool) {
        let (point, at) = match self.task_kind_mut(task) {
            Some(TaskKind::Hover { point, at }) => (*point, *at),
            _ => unreachable!(),
        };
        let Some(at) = at else {
            return (false, false);
        };
        let Some(course) = self.bricks[at].course else {
            return (false, false);
        };
        let Some(point) = point else {
            self.hover_brick = None;
            return (false, false);
        };
        let ci = self.courses[course].index;
        if point.transverse < self.courses[course].transverse_start && ci > 0 {
            let prev = self.wall.children[ci - 1];
            let b = self.courses[prev].children[0];
            if let Some(TaskKind::Hover { at, .. }) = self.task_kind_mut(task) {
                *at = Some(b);
            }
            return (true, false);
        } else if ci < self.wall.children.len() - 1
            && point.transverse > self.courses[self.wall.children[ci + 1]].transverse_start
        {
            let next = self.wall.children[ci + 1];
            let b = self.courses[next].children[0];
            if let Some(TaskKind::Hover { at, .. }) = self.task_kind_mut(task) {
                *at = Some(b);
            }
            return (true, false);
        } else {
            let mut at = at;
            while point.converse < self.brick_get_converse(at) && self.bricks[at].index > 0 {
                at = self.courses[course].children[self.bricks[at].index - 1];
            }
            while point.converse >= self.brick_converse_edge(at)
                && self.bricks[at].index < self.courses[course].children.len() - 1
            {
                at = self.courses[course].children[self.bricks[at].index + 1];
            }
            let old = self.hover;
            let hover0 = self.brick_hover(at, point);
            let hover_changed;
            match hover0 {
                None => {
                    hover_changed = old.is_some();
                    self.hover = None;
                }
                Some((h, changed)) => {
                    hover_changed = changed;
                    self.hover = Some(h);
                }
            }
            if hover_changed {
                if let Some(o) = old {
                    if self.hover != Some(o) {
                        self.hoverable_clear(o);
                    }
                }
            }
            self.hover_brick = Some(at);
            if let Some(TaskKind::Hover { at: a, .. }) = self.task_kind_mut(task) {
                *a = Some(at);
            }
            return (false, hover_changed);
        }
    }

    fn run_hover(&mut self, task: TaskId) -> bool {
        let (more, changed) = self.hover_inner(task);
        if changed {
            self.hover_changed();
        }
        return more;
    }

    /// Merman's mouse move listener: remember the point for the hover task.
    pub fn mouse_moved(&mut self, point: Vector) {
        if self.hover_idle.is_none() {
            let at = match self.hover_brick {
                Some(b) => Some(b),
                None => self
                    .wall
                    .children
                    .first()
                    .map(|c| self.courses[*c].children[0]),
            };
            let t = self.add_iteration(TaskKind::Hover { point: None, at }, P::HOVER);
            self.hover_idle = Some(t);
        }
        let t = self.hover_idle.unwrap();
        if let Some(TaskKind::Hover { point: p, .. }) = self.task_kind_mut(t) {
            *p = Some(point);
        }
    }

    pub fn mouse_exited(&mut self) {
        if let Some(t) = self.hover_idle {
            if let Some(TaskKind::Hover { point, .. }) = self.task_kind_mut(t) {
                *point = None;
            }
        } else if self.hover.is_some() {
            self.clear_hover();
        }
    }

    /// The cornerstone brick changed: track it for scrolling.
    pub fn cornerstone_changed(&mut self, brick: BrickId) {
        for b in 0..self.bricks.len() {
            if self.bricks[b].attachments.contains(&crate::attachment::AttachmentRef::Cornerstone) {
                self.brick_remove_attachment(b, crate::attachment::AttachmentRef::Cornerstone);
            }
        }
        self.brick_add_attachment(brick, crate::attachment::AttachmentRef::Cornerstone);
    }
}
