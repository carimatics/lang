use super::*;
impl Target {
    fn paused_frame(&self, args: &Value) -> Result<usize, String> {
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        let id = args["frameId"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("invalid frameId")?;
        self.frames
            .iter()
            .position(|(frame, _)| *frame == id)
            .ok_or("frame belongs to another pause".into())
    }
    fn inspection(&self, frame: usize, query: Query) -> Result<Vec<Preview>, String> {
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        let (reply, receive) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or("debug worker unavailable")?
            .try_send(Command::Inspect {
                frame,
                query,
                reply,
            })
            .map_err(|_| "debug worker unavailable")?;
        receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "inspection did not complete")?
    }
    fn reference(&mut self, value: Reference) -> Result<u32, String> {
        if self.next_reference >= REFERENCES {
            return Err("debug reference limit exceeded".into());
        }
        let bytes = value.bytes();
        if self
            .index_bytes
            .saturating_add(self.reference_bytes)
            .saturating_add(bytes)
            > INDEX_BYTES
        {
            return Err("debug reference byte limit exceeded".into());
        }
        let id = self.next_reference;
        self.next_reference += 1;
        self.references.insert(id, value);
        self.reference_bytes += bytes;
        Ok(id)
    }
    pub fn scopes(&mut self, args: &Value) -> Result<Value, String> {
        let frame = self.paused_frame(args)?;
        let reference = self.reference(Reference::Locals(frame))?;
        Ok(
            json!({"scopes":[{"name":"Locals","presentationHint":"locals","variablesReference":reference,"expensive":false}]}),
        )
    }
    pub fn variables(&mut self, args: &Value) -> Result<Value, String> {
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        let reference = args["variablesReference"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .and_then(|n| self.references.get(&n))
            .cloned()
            .ok_or("reference belongs to another pause")?;
        match args.get("filter").and_then(Value::as_str) {
            Some("indexed") => return Ok(json!({"variables":[]})),
            Some("named") | None => {}
            _ => return Err("invalid variable filter".into()),
        }
        let index = |name: &str, default: usize| -> Result<usize, String> {
            args.get(name)
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| format!("invalid {name}"))
                })
                .unwrap_or(Ok(default))
        };
        let start = index("start", 0)?;
        let count = index("count", 1024)?;
        let count = if count == 0 { 1024 } else { count };
        if count > 1024 {
            return Err("variable response limit exceeded".into());
        }
        let (frame, path, query) = match reference {
            Reference::Locals(frame) => (frame, Vec::new(), Query::Locals),
            Reference::Value(frame, path) => (frame, path.clone(), Query::Children(path)),
        };
        let previews = self.inspection(frame, query)?;
        let selected: Vec<_> = previews.into_iter().skip(start).take(count).collect();
        let needed = selected.iter().filter(|v| v.children > 0).count();
        if self.next_reference as usize + needed >= REFERENCES as usize {
            return Err("debug reference limit exceeded".into());
        }
        let mut bytes = 0usize;
        let mut response_bytes = 1024usize;
        for preview in &selected {
            let mut child_path = path.clone();
            child_path.push(preview.name.clone());
            let text_bytes = child_path.iter().map(String::len).sum::<usize>()
                + child_path.len().saturating_sub(1);
            response_bytes = response_bytes.saturating_add(256).saturating_add(
                (preview.name.len() + preview.display.len() + preview.type_name.len() + text_bytes)
                    .saturating_mul(6),
            );
            if preview.children > 0 {
                if child_path.len() > 32 || text_bytes > 4096 {
                    return Err("inspection path limit exceeded".into());
                }
                bytes = bytes.saturating_add(Reference::Value(frame, child_path).bytes());
            }
        }
        if response_bytes > 4 * 1024 * 1024
            || self
                .index_bytes
                .saturating_add(self.reference_bytes)
                .saturating_add(bytes)
                > INDEX_BYTES
        {
            return Err("inspection response/reference limit exceeded".into());
        }
        let mut variables = Vec::new();
        for preview in selected {
            let mut child_path = path.clone();
            child_path.push(preview.name.clone());
            let reference = if preview.children > 0 {
                self.reference(Reference::Value(frame, child_path.clone()))?
            } else {
                0
            };
            variables.push(json!({"name":preview.name,"value":preview.display,"type":preview.type_name,
                "variablesReference":reference,"namedVariables":preview.children,"evaluateName":child_path.join("."),
                "presentationHint":{"attributes":["readOnly"]}}));
        }
        Ok(json!({"variables":variables}))
    }
    pub fn evaluate(&mut self, args: &Value) -> Result<Value, String> {
        let frame = if args.get("frameId").is_some() {
            self.paused_frame(args)?
        } else if self.state == State::Paused {
            0
        } else {
            return Err("target is not paused".into());
        };
        let expression = text(args, "expression")?;
        if expression.is_empty() || expression.len() > 4096 {
            return Err("evaluation path limit exceeded".into());
        }
        let path: Vec<_> = expression.split('.').map(str::to_owned).collect();
        if path.len() > 32 || path.iter().any(String::is_empty) {
            return Err("invalid evaluation path".into());
        }
        let mut result = self.inspection(frame, Query::Evaluate(path.clone()))?;
        let preview = result.pop().ok_or("evaluation unavailable")?;
        let reference = if preview.children > 0 {
            self.reference(Reference::Value(frame, path))?
        } else {
            0
        };
        Ok(
            json!({"result":preview.display,"type":preview.type_name,"variablesReference":reference,"namedVariables":preview.children,
            "presentationHint":{"attributes":["readOnly"]}}),
        )
    }
}
