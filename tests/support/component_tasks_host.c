/* Root tasks use the original structured tree and generated typed C calls. */
static void task_nested_refusal(const lang_component_v1_api *api,const lang_component_v1_context *context,lang_component_v1_binding *binding) {
    lang_component_v1_error error={0};
    lang_component_v1_result *result=(lang_component_v1_result *)(uintptr_t)42;
    lang_component_v1_bytes input;
    @TASK_ARGUMENTS@ arguments={0};lc_value_writer writer={0};uint8_t scratch[1024];
    error.header.size=sizeof(error);error.header.version=1;
    writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
    writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
    assert(@TASK_NAME@_encode_arguments(&writer,&arguments));input.data=scratch;input.length=writer.at;
    assert(api->context_call(context,binding,input,&result,&error)==4);
    assert(result==(lang_component_v1_result *)(uintptr_t)42);
    assert(api->context_check(context,&error)==0);
}

static void task_root_case(const lang_component_v1_api *api,const lang_component_v1_config *config,uint32_t format) {
    lang_component_v1_engine *engine=NULL;
    lang_component_v1_module *module=NULL;
    lang_component_v1_instance *instance=NULL;
    lang_component_v1_binding *binding=NULL;
    lang_component_v1_instance_config options={0};
    lang_component_v1_error error={0};
    lang_component_v1_bytes code={format?task_artifact:task_source,format?sizeof(task_artifact):sizeof(task_source)};
    lang_component_v1_bytes metadata={format?NULL:task_idl,format?0:sizeof(task_idl)};
    lang_component_v1_bytes projection={task_root_projection,sizeof(task_root_projection)};
    uint32_t profile;uint64_t fuel=0,prefix;
    error.header.size=sizeof(error);error.header.version=1;
    options.header.size=sizeof(options);options.header.version=1;options.ceiling=config->ceiling;
    assert(api->engine_create(config,&engine,&error)==0);
    assert(api->module_load(engine,format,code,metadata,&module,&error)==0);
    assert(api->instance_create(module,&options,&instance,&error)==0);
    for(profile=0;profile<sizeof(task_refusals)/sizeof(task_refusals[0]);++profile) {
        lang_component_v1_binding *refused=(lang_component_v1_binding *)(uintptr_t)42;
        assert(api->bind_export(instance,task_refusals[profile],&refused,&error)==2);
        assert(refused==(lang_component_v1_binding *)(uintptr_t)42);
    }
    assert(api->bind_export(instance,projection,&binding,&error)==0);
    assert(api->module_drop(module,&error)==0);
    for(profile=0;profile<4;++profile) {
        lang_component_v1_control *control=NULL;
        lang_component_v1_limits limits=config->ceiling;
        lang_component_v1_usage usage={0};
        lang_component_v1_bytes bytes;
        @TASK_ARGUMENTS@ arguments={0};@TASK_RESULT@ value=0;
        lc_value_writer writer={0};lc_value_reader reader={0};lc_value_outcome result;
        uint8_t scratch[1024];
        if(profile==1)limits.live_tasks=1;
        assert(api->control_create(engine,&limits,&control,&error)==0);
        if(profile==2)api->interrupt(control);
        writer.data=scratch;writer.capacity=sizeof(scratch);
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        result=@TASK_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(result.status==(profile==1||profile==2?1u:0u)&&result.result&&!result.poisoned);
        usage.header.size=sizeof(usage);usage.header.version=1;
        assert(api->result_usage(result.result,&usage,&error)==0);
        assert(usage.peak_live_tasks==(profile==1?1u:profile==2?0u:2u));
        if(profile==0)fuel=usage.fuel;
        assert(api->result_bytes(result.result,&bytes,&error)==0);
        if(profile==0||profile==3) {
            reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
            reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
            assert(@TASK_NAME@_decode_result(&reader,&value)&&value==42);
        } else assert(probe_task_report(bytes,config->mode,profile==1?0u:1u,profile==1?1u:0u)==0);
        assert(api->result_drop(result.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    assert(fuel>0&&fuel<1000);
    for(prefix=0;prefix<fuel;++prefix) {
        lang_component_v1_control *control=NULL;lang_component_v1_limits limits=config->ceiling;
        lang_component_v1_bytes bytes;lc_value_outcome result;
        @TASK_ARGUMENTS@ arguments={0};lc_value_writer writer={0};uint8_t scratch[1024];
        limits.fuel=prefix;assert(api->control_create(engine,&limits,&control,&error)==0);
        writer.data=scratch;writer.capacity=sizeof(scratch);
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        result=@TASK_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(result.status==1&&result.result&&!result.poisoned);
        assert(api->result_bytes(result.result,&bytes,&error)==0);
        assert(probe_task_report(bytes,config->mode,2,prefix)==0);
        assert(api->result_drop(result.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    {
        lang_component_v1_control *control=NULL;lc_value_outcome result;
        @TASK_ARGUMENTS@ arguments={0};lc_value_writer writer={0};uint8_t scratch[1024];
        assert(api->control_create(engine,&config->ceiling,&control,&error)==0);
        writer.data=scratch;writer.capacity=sizeof(scratch);
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        result=@TASK_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(result.status==0&&result.result&&!result.poisoned);
        assert(api->result_drop(result.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    assert(api->binding_drop(binding,&error)==0);
    {
        lang_component_v1_result *closed=NULL;
        assert(api->instance_close(instance,&closed,&error)==0);
        assert(closed==NULL);
    }
    assert(api->engine_destroy(engine,&error)==0);
}
