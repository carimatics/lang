/* Original-table resource publication, scoped borrow and exact retirement. */
typedef struct {
    const lang_component_v1_api *api;
    lang_component_v1_engine *engine;
    lang_component_v1_instance *busy_instance;
    lang_component_v1_binding *callback;
    lang_component_v1_binding *relay_callback;
    uintptr_t auxiliary;
    uint32_t auxiliary_destroyed;
    uintptr_t backing;
    uint32_t calls,destroyed,released,fail;
    uint32_t depth,profile;
} ResourceOwner;
static void resource_release_owner(void *user) {
    ResourceOwner *owner=(ResourceOwner *)user;assert(owner->released==0);owner->released++;
}
static uint32_t resource_destroy(void *user,const lang_component_v1_context *context,
                                 lang_component_v1_mut_bytes bytes,lang_component_v1_buffer **failure) {
    ResourceOwner *owner=(ResourceOwner *)user;lang_component_v1_error error={0};int64_t value;
    error.header.size=sizeof(error);error.header.version=1;
    assert(context->engine==owner->engine&&owner->api->context_check(context,&error)==0);
    assert(bytes.length==8&&((uintptr_t)bytes.data%64)==0&&*failure==NULL);
    memcpy(&value,bytes.data,8);
    if(value>=1000){assert((uintptr_t)bytes.data==owner->auxiliary);owner->auxiliary_destroyed++;return 0;}
    {
        lang_component_v1_result *unchanged=(lang_component_v1_result *)(uintptr_t)42;
        assert(owner->api->resource_drop(owner->busy_instance,1,&unchanged,&error)==4);
        assert(unchanged==(lang_component_v1_result *)(uintptr_t)42);
    }
    if(owner->destroyed==0)assert(value==41+(int64_t)owner->calls&&(uintptr_t)bytes.data==owner->backing);
    else if(value==91)assert(owner->destroyed==4);
    else assert(value==80-(int64_t)owner->destroyed);
    owner->destroyed++;return ((owner->destroyed==1||value==79||value==78)&&owner->fail)?1:0;
}
static uint32_t resource_touch(void *user,const lang_component_v1_context *context,
                               lang_component_v1_bytes input,lang_component_v1_buffer **output) {
    ResourceOwner *owner=(ResourceOwner *)user;lang_component_v1_error error={0};
    lang_component_v1_mut_bytes backing,buffer;uint8_t scratch[1024];
    @RESOURCE_HOST_ARGS@ arguments={0};@RESOURCE_HOST_RESULT@ result=0;
    lc_value_reader reader={0};lc_value_writer writer={0};
    error.header.size=sizeof(error);error.header.version=1;
    assert(context->engine==owner->engine&&owner->api->context_check(context,&error)==0);
    reader.data=input.data;reader.length=input.length;reader.contextual=1;
    reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
    assert(@RESOURCE_HOST_NAME@_decode_arguments(&reader,&arguments));
    assert(owner->api->context_resource_write(context,arguments.p0.index,&backing,&error)==0);
    assert(backing.length==8&&((uintptr_t)backing.data%64)==0);
    if(owner->calls)assert(owner->backing==(uintptr_t)backing.data);
    owner->backing=(uintptr_t)backing.data;memcpy(&result,backing.data,8);
    assert(result==41+(int64_t)owner->calls);result++;memcpy(backing.data,&result,8);owner->calls++;
    if(!owner->depth) {
        uint32_t i;owner->depth++;
        for(i=0;i<(owner->profile?2u:1u);++i) {
            @RESOURCE_ROOT_ARGS@ args={0};@RESOURCE_ROOT_RESULT@ value=0;
            lc_value_writer child_writer={0};lc_value_reader child_reader={0};
            lc_value_outcome child;lang_component_v1_bytes bytes;
            uint8_t child_scratch[1024];args.p0.index=arguments.p0.index;
            child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
            child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
            child=@RESOURCE_ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&child_writer,&error);
            if(!owner->profile){assert(child.status==4&&!child.result);break;}
            assert(child.status==0&&child.commit==1&&child.result);
            assert(owner->api->context_check(context,&error)==0);
            assert(owner->api->context_resource_read(context,arguments.p0.index,&bytes,&error)==0);
            assert(bytes.length==8&&(uintptr_t)bytes.data==owner->backing);
            assert(owner->api->result_bytes(child.result,&bytes,&error)==0);
            child_reader.data=bytes.data;child_reader.length=bytes.length;child_reader.contextual=1;
            child_reader.left.work=1000000;child_reader.left.elements=1000;child_reader.left.depth=128;
            assert(@RESOURCE_ROOT_NAME@_decode_result(&child_reader,&value));
            assert(value==41+(int64_t)owner->calls);result=value;
            {lang_component_v1_usage usage={0};usage.header.size=sizeof(usage);usage.header.version=1;
             assert(owner->api->result_usage(child.result,&usage,&error)==0&&usage.canonical_indices==1&&usage.host_calls==2+i);}
            assert(owner->api->result_drop(child.result,&error)==0);
        }
        owner->depth--;
        if(owner->profile) {
            lang_component_v1_buffer *published=NULL;lang_component_v1_mut_bytes data;
            lang_component_v1_bytes key={resource_key,sizeof(resource_key)};
            uint32_t index=0,j;int64_t value=1000+(int64_t)owner->auxiliary_destroyed;
            assert(owner->api->context_buffer_create(context,8,64,&published,&error)==0);
            assert(owner->api->buffer_mut_bytes(context,published,&data,&error)==0&&data.length==8);
            memcpy(data.data,&value,8);owner->auxiliary=(uintptr_t)data.data;
            {lang_component_v1_bytes bad={(const uint8_t *)"bad",3};uint32_t untouched=42;
             assert(owner->api->context_resource_publish(context,bad,published,&untouched,&error)==2&&untouched==42);
             assert(owner->api->buffer_mut_bytes(context,published,&data,&error)==0&&memcmp(data.data,&value,8)==0);}
            assert(owner->api->context_resource_publish(context,key,published,&index,&error)==0&&index==2);
            for(j=0;j<2;++j) {
                @RESOURCE_RELAY_ARGS@ args={0};@RESOURCE_RELAY_RESULT@ returned={0};
                lc_value_writer child_writer={0};lc_value_reader child_reader={0};lc_value_outcome child;
                lang_component_v1_bytes bytes;uint8_t child_scratch[1024];uint32_t old=index;
                args.p0.index=index;child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
                child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
                child=@RESOURCE_RELAY_NAME@_context_call(owner->api,context,owner->relay_callback,&args,&child_writer,&error);
                if(owner->profile==3&&j==1) {
                    assert(child.status==1&&child.commit==1&&!child.poisoned&&child.result);
                    assert(owner->api->result_bytes(child.result,&bytes,&error)==0&&bytes.length>4&&memcmp(bytes.data,"LCMP",4)==0);
                    assert(owner->api->result_drop(child.result,&error)==0);
                    assert(owner->api->context_resource_read(context,old,&bytes,&error)==2);
                    break;
                }
                assert(child.status==0&&child.commit==1&&child.result);
                assert(owner->api->result_bytes(child.result,&bytes,&error)==0);
                child_reader.data=bytes.data;child_reader.length=bytes.length;child_reader.contextual=1;
                child_reader.left.work=1000000;child_reader.left.elements=1000;child_reader.left.depth=128;
                assert(@RESOURCE_RELAY_NAME@_decode_result(&child_reader,&returned));index=returned.index;
                assert(index==old+1);
                assert(owner->api->result_drop(child.result,&error)==0);
                assert(owner->auxiliary_destroyed==0||owner->auxiliary_destroyed==1);
                assert(owner->api->context_resource_read(context,old,&bytes,&error)==2);
                assert(owner->api->context_resource_read(context,index,&bytes,&error)==0);
                assert(bytes.length==8&&(uintptr_t)bytes.data==owner->auxiliary);
                assert(memcmp(bytes.data,&value,8)==0);
            }
        }
    }
    writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
    writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
    assert(@RESOURCE_HOST_NAME@_encode_result(&writer,&result));
    assert(owner->api->context_buffer_create(context,writer.at,1,output,&error)==0);
    assert(owner->api->buffer_mut_bytes(context,*output,&buffer,&error)==0);
    memcpy(buffer.data,scratch,(size_t)writer.at);return 0;
}
static void resource_profile(const lang_component_v1_api *api,const lang_component_v1_config *original,uint32_t format,uint32_t fail,uint32_t profile) {
    ResourceOwner owner={0};lang_component_v1_config config=*original;
    lang_component_v1_engine *engine=NULL;lang_component_v1_module *module=NULL;
    lang_component_v1_provider *provider=NULL;lang_component_v1_instance *a=NULL,*b=NULL,*c=NULL;
    lang_component_v1_binding *binding=NULL;lang_component_v1_error error={0};
    lang_component_v1_binding *callback=NULL,*relay_callback=NULL;
    lang_component_v1_provider_config registration={0};lang_component_v1_import method={0};
    lang_component_v1_resource_type type={0};lang_component_v1_instance_config options={0};
    lang_component_v1_bytes host={resource_host_contract,sizeof(resource_host_contract)};
    lang_component_v1_bytes grant={resource_grant,sizeof(resource_grant)},request={resource_projection,sizeof(resource_projection)};
    lang_component_v1_bytes program={format?resource_artifact:resource_program,format?sizeof(resource_artifact):sizeof(resource_program)};
    lang_component_v1_bytes idl_input={format?NULL:resource_idl,format?0:sizeof(resource_idl)};
    lang_component_v1_bytes key={resource_key,sizeof(resource_key)},input;
    lang_component_v1_result *closed=(lang_component_v1_result *)(uintptr_t)42,*report=NULL,*close_report=NULL;
    uint32_t first=0,second=0,i,untouched=42;int64_t value=41;
    config.ceiling.preparation_work=10000000;config.ceiling.metadata_bytes=16777216;
    config.ceiling.serialization_bytes=16777216;config.ceiling.value_elements=1000000;
    if(profile)config.ceiling.canonical_indices=8;
    error.header.size=sizeof(error);error.header.version=1;
    assert(api->engine_create(&config,&engine,&error)==0);owner.api=api;owner.engine=engine;owner.fail=fail;owner.profile=profile;
    method.header.size=sizeof(method);method.header.version=1;
    method.function_key.data=resource_method_key;method.function_key.length=sizeof(resource_method_key);
    memcpy(method.signature,resource_method_signature,32);method.invoke=resource_touch;method.user=&owner;
    method.flags=profile?1u:0u;
    type.header.size=sizeof(type);type.header.version=1;type.resource_key=key;type.destroy=resource_destroy;type.user=&owner;
    registration.header.size=sizeof(registration);registration.header.version=1;
    registration.contract=host;registration.imports=&method;registration.import_count=1;
    registration.resource_types=&type;registration.resource_type_count=1;registration.owner=&owner;registration.release_owner=resource_release_owner;
    assert(api->provider_register(engine,&registration,&provider,&error)==0);
    assert(api->module_load(engine,format,program,idl_input,&module,&error)==0);
    options.header.size=sizeof(options);options.header.version=1;options.ceiling=config.ceiling;options.grants=&grant;options.grant_count=1;
    assert(api->instance_create(module,&options,&a,&error)==0);
    assert(api->instance_create(module,&options,&b,&error)==0);
    options.ceiling.canonical_indices=1;
    assert(api->instance_create(module,&options,&c,&error)==0);
    assert(api->bind_export(a,request,&binding,&error)==0);
    assert(api->bind_export(profile>=2?b:a,request,&callback,&error)==0);owner.callback=callback;
    {lang_component_v1_bytes projection={resource_relay_projection,sizeof(resource_relay_projection)};
     assert(api->bind_export(profile>=2?b:a,projection,&relay_callback,&error)==0);owner.relay_callback=relay_callback;}
    assert(api->module_drop(module,&error)==0);
    input.data=(const uint8_t *)&value;input.length=8;
    assert(api->resource_publish(a,provider,key,input,3,&untouched,&error)==2&&untouched==42);
    assert(owner.destroyed==0&&owner.released==0);
    assert(api->resource_publish(a,provider,key,input,64,&first,&error)==0&&first!=0);
    value=77;assert(api->resource_publish(b,provider,key,input,64,&second,&error)==0&&second!=0);
    value=78;assert(api->resource_publish(b,provider,key,input,64,&second,&error)==0&&second==2);
    value=79;assert(api->resource_publish(b,provider,key,input,64,&second,&error)==0&&second==3);
    value=999;
    {
        lang_component_v1_limits limited=config.ceiling;lang_component_v1_control *control=NULL;
        @RESOURCE_ROOT_ARGS@ arguments={0};lc_value_writer writer={0};lc_value_outcome outcome;
        uint8_t scratch[1024];limited.canonical_indices=0;arguments.p0.index=first;
        assert(api->control_create(engine,&limited,&control,&error)==0);
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        outcome=@RESOURCE_ROOT_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(outcome.status==1&&outcome.commit==0&&outcome.poisoned==0&&outcome.result);
        assert(owner.calls==0&&owner.destroyed==0);
        assert(api->result_drop(outcome.result,&error)==0&&api->control_drop(control,&error)==0);
    }
    for(i=0;i<2;++i) {
        lang_component_v1_control *control=NULL;@RESOURCE_ROOT_ARGS@ arguments={0};@RESOURCE_ROOT_RESULT@ result=0;
        lang_component_v1_limits callback_limits=config.ceiling;
        lc_value_writer writer={0};lc_value_reader reader={0};lc_value_outcome outcome;
        lang_component_v1_bytes bytes;uint8_t scratch[1024];arguments.p0.index=first;
        if(profile==3)callback_limits.canonical_indices=3;
        assert(api->control_create(engine,&callback_limits,&control,&error)==0);
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        outcome=@RESOURCE_ROOT_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(outcome.status==0&&outcome.commit==1&&outcome.result);
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        assert(@RESOURCE_ROOT_NAME@_decode_result(&reader,&result)&&result==(profile?44+3*(int64_t)i:42+(int64_t)i));
        assert(api->result_drop(outcome.result,&error)==0&&api->control_drop(control,&error)==0);
    }
    assert(owner.calls==(profile?6u:2u)&&owner.destroyed==0);
    assert(owner.auxiliary_destroyed==(profile?2u:0u));
    {
        lang_component_v1_binding *relay=NULL;lang_component_v1_control *control=NULL;
        lang_component_v1_bytes projection={resource_relay_projection,sizeof(resource_relay_projection)},bytes;
        @RESOURCE_RELAY_ARGS@ arguments={0};@RESOURCE_RELAY_RESULT@ result={0};
        lc_value_writer writer={0};lc_value_reader reader={0};lc_value_outcome outcome;uint8_t scratch[1024];
        assert(api->bind_export(a,projection,&relay,&error)==0);
        assert(api->control_create(engine,&config.ceiling,&control,&error)==0);
        arguments.p0.index=first;writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        outcome=@RESOURCE_RELAY_NAME@_call(api,control,relay,&arguments,&writer,&error);
        assert(outcome.status==0&&outcome.commit==1&&outcome.result);
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        assert(@RESOURCE_RELAY_NAME@_decode_result(&reader,&result)&&result.index&&result.index!=first);
        assert(api->result_drop(outcome.result,&error)==0&&owner.destroyed==0);
        assert(api->resource_drop(a,first,&closed,&error)==5&&closed==(lang_component_v1_result *)(uintptr_t)42);
        first=result.index;
        assert(api->control_drop(control,&error)==0&&api->binding_drop(relay,&error)==0);
    }
    owner.busy_instance=a;
    assert(api->resource_drop(a,first,&report,&error)==fail);
    assert(owner.destroyed==1);if(fail) {
        lang_component_v1_result_info info={0};lang_component_v1_bytes bytes;
        info.header.size=sizeof(info);info.header.version=1;
        assert(report&&api->result_info(report,&info,&error)==0&&info.kind==1&&info.flags==3);
        assert(api->result_bytes(report,&bytes,&error)==0&&bytes.length>0);
    } else assert(report==NULL);
    assert(api->resource_drop(a,first,&closed,&error)==5&&closed==(lang_component_v1_result *)(uintptr_t)42&&owner.destroyed==1);
    assert(api->instance_close(a,&closed,&error)==0&&closed==NULL&&owner.destroyed==1);
    owner.busy_instance=b;
    assert(api->instance_close(b,&close_report,&error)==fail&&owner.destroyed==4);
    if(fail) {
        lang_component_v1_result_info info={0};lang_component_v1_usage usage={0};lang_component_v1_bytes bytes;
        info.header.size=sizeof(info);info.header.version=1;usage.header.size=sizeof(usage);usage.header.version=1;
        assert(close_report&&api->result_info(close_report,&info,&error)==0&&info.kind==1&&info.flags==3);
        assert(info.suppressed_count==1&&info.omitted_records==0);
        assert(api->result_usage(close_report,&usage,&error)==0&&usage.canonical_indices==3);
        assert(api->result_bytes(close_report,&bytes,&error)==0&&bytes.length>0);
    } else assert(close_report==NULL);
    {
        lang_component_v1_binding *relay=NULL;lang_component_v1_control *control=NULL;
        lang_component_v1_bytes projection={resource_relay_projection,sizeof(resource_relay_projection)};
        @RESOURCE_RELAY_ARGS@ arguments={0};lc_value_writer writer={0};lc_value_outcome outcome;
        lang_component_v1_result_info info={0};uint8_t scratch[1024];uint32_t index=0;
        value=91;assert(api->resource_publish(c,provider,key,input,64,&index,&error)==0&&index==1);
        assert(api->resource_publish(c,provider,key,input,64,&untouched,&error)==2&&untouched==42);
        assert(error.limit_kind==8&&error.limit==1&&error.attempted==2);
        assert(api->bind_export(c,projection,&relay,&error)==0);
        assert(api->control_create(engine,&config.ceiling,&control,&error)==0);
        owner.busy_instance=c;arguments.p0.index=index;
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        outcome=@RESOURCE_RELAY_NAME@_call(api,control,relay,&arguments,&writer,&error);
        assert(outcome.status==1&&outcome.commit==1&&outcome.result&&owner.destroyed==5);
        info.header.size=sizeof(info);info.header.version=1;
        assert(api->result_info(outcome.result,&info,&error)==0&&info.kind==1&&(info.flags&1));
        {lang_component_v1_bytes bytes;assert(api->result_bytes(outcome.result,&bytes,&error)==0&&bytes.length>0);}
        assert(api->result_drop(outcome.result,&error)==0&&api->control_drop(control,&error)==0);
        assert(api->resource_drop(c,index,&closed,&error)==5&&closed==NULL);
        assert(api->instance_close(c,&closed,&error)==0&&closed==NULL&&owner.destroyed==5);
        assert(api->binding_drop(relay,&error)==0);
    }
    assert(api->binding_drop(binding,&error)==0);
    assert(api->binding_drop(callback,&error)==0);
    assert(api->binding_drop(relay_callback,&error)==0);
    assert(api->provider_drop(provider,&closed,&error)==0&&closed==NULL);
    if(fail){assert(owner.released==0);assert(api->result_drop(report,&error)==0);assert(owner.released==0);assert(api->result_drop(close_report,&error)==0);}
    assert(owner.released==1&&api->engine_destroy(engine,&error)==0);
}
static void resource_case(const lang_component_v1_api *api,const lang_component_v1_config *config,uint32_t format,uint32_t fail) {
    uint32_t profile;for(profile=0;profile<4;++profile)resource_profile(api,config,format,fail,profile);
}
