use crate::format::ioi::*;
use crate::format::*;

pub type IOIBatchSubtaskId = u32;
pub type IOIBatchTestcaseId = u32;

pub struct IOIBatchTask {
    pub info: IOITaskInfo,
}

pub struct IOIBatchSubtaskInfo;

pub struct IOIBatchTestcaseInfo;

pub struct IOIBatchGenerator;

pub struct IOIBatchValidator;

pub struct IOIBatchSolution;

pub struct IOIBatchChecker;

impl Generator<IOIBatchSubtaskId, IOIBatchTestcaseId> for IOIBatchGenerator {
    fn generate(
        &self,
        dag: &mut ExecutionDAG,
        subtask: IOIBatchSubtaskId,
        testcase: IOIBatchTestcaseId,
    ) -> File {
        unimplemented!();
    }
}

impl Validator<IOIBatchSubtaskId, IOIBatchTestcaseId> for IOIBatchValidator {
    fn validate(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        subtask: IOIBatchSubtaskId,
        testcase: IOIBatchTestcaseId,
    ) -> File {
        unimplemented!();
    }
}

impl Solution<IOIBatchSubtaskId, IOIBatchTestcaseId> for IOIBatchSolution {
    fn solve(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        validation: Option<File>,
        subtask: IOIBatchSubtaskId,
        testcase: IOIBatchTestcaseId,
    ) -> File {
        unimplemented!();
    }
}

impl<F> Checker<IOIBatchSubtaskId, IOIBatchTestcaseId, F> for IOIBatchChecker
where
    F: Fn(CheckerResult) -> () + 'static,
{
    fn check(
        &self,
        dag: &mut ExecutionDAG,
        input: File,
        output: Option<File>,
        test: File,
        subtask: IOIBatchSubtaskId,
        testcase: IOIBatchTestcaseId,
        callback: F,
    ) {
        unimplemented!();
    }
}

impl TestcaseInfo for IOIBatchTestcaseInfo {
    fn write_input_to(&self) -> Option<PathBuf> {
        unimplemented!();
    }
    fn write_output_to(&self) -> Option<PathBuf> {
        unimplemented!();
    }
}

impl SubtaskInfo for IOIBatchSubtaskInfo {
    fn max_score(&self) -> f64 {
        unimplemented!();
    }
    fn score_mode(&self) {
        unimplemented!();
    }
}

impl Task for IOIBatchTask {
    type SubtaskId = IOIBatchSubtaskId;
    type TestcaseId = IOIBatchTestcaseId;
    type SubtaskInfo = IOIBatchSubtaskInfo;
    type TestcaseInfo = IOIBatchTestcaseInfo;

    fn format() -> &'static str {
        "ioi-batch"
    }

    fn name(&self) -> String {
        unimplemented!();
    }

    fn title(&self) -> String {
        unimplemented!();
    }

    fn subtasks(&self) -> HashMap<Self::SubtaskId, Self::SubtaskInfo> {
        unimplemented!();
    }

    fn testcases(&self, subtask: Self::SubtaskId) -> HashMap<Self::TestcaseId, Self::TestcaseInfo> {
        unimplemented!();
    }

    fn solutions(&self) -> HashMap<PathBuf, &Solution<Self::SubtaskId, Self::TestcaseId>> {
        unimplemented!();
    }

    fn generator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Box<Generator<Self::SubtaskId, Self::TestcaseId>> {
        Box::new(StaticFileProvider::new(
            format!("Static input of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        ))
    }

    fn validator(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Validator<Self::SubtaskId, Self::TestcaseId>>> {
        Some(Box::new(IOIBatchValidator {}))
    }

    fn official_solution(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Option<Box<Solution<Self::SubtaskId, Self::TestcaseId>>> {
        Some(Box::new(StaticFileProvider::new(
            format!("Static output of testcase {}", testcase),
            std::path::Path::new(".").to_owned(),
        )))
    }

    fn checker<F>(
        &self,
        subtask: Self::SubtaskId,
        testcase: Self::TestcaseId,
    ) -> Box<Checker<Self::SubtaskId, Self::TestcaseId, F>> {
        unimplemented!();
    }
}