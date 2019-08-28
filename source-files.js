var N = null;var sourcesIndex = {};
sourcesIndex["task_maker"] = {"name":"","files":["main.rs","opt.rs"]};
sourcesIndex["task_maker_cache"] = {"name":"","files":["lib.rs"]};
sourcesIndex["task_maker_dag"] = {"name":"","files":["dag.rs","execution.rs","file.rs","lib.rs","signals.rs"]};
sourcesIndex["task_maker_exec"] = {"name":"","dirs":[{"name":"executors","files":["local_executor.rs","mod.rs"]}],"files":["check_dag.rs","client.rs","executor.rs","lib.rs","proto.rs","sandbox.rs","scheduler.rs","worker.rs","worker_manager.rs"]};
sourcesIndex["task_maker_format"] = {"name":"","dirs":[{"name":"ioi","dirs":[{"name":"format","dirs":[{"name":"italian_yaml","files":["gen_gen.rs","mod.rs","static_inputs.rs"]}],"files":["mod.rs"]}],"files":["curses_ui.rs","dag.rs","finish_ui.rs","mod.rs","print.rs","tag.rs","ui_state.rs"]},{"name":"ui","files":["json.rs","mod.rs","raw.rs"]}],"files":["lib.rs","source_file.rs"]};
sourcesIndex["task_maker_lang"] = {"name":"","dirs":[{"name":"languages","files":["c.rs","cpp.rs","mod.rs","python.rs","shell.rs"]}],"files":["grader_map.rs","lib.rs","source_file.rs"]};
sourcesIndex["task_maker_store"] = {"name":"","files":["lib.rs","read_file_iterator.rs"]};
createSourceSidebar();