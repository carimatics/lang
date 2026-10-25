/* Native C registry fixture: no public version-1 query/table. */
typedef struct {
    const lang_component_v1_api *api;
    lang_component_v1_engine *engine;
    lang_component_v1_provider *provider;
    lang_component_v1_context saved;
    lang_component_v1_context inner;
    lang_component_v1_binding *callback;
    lang_component_v1_result *retained;
    lang_component_v1_control *control;
    uint32_t depth, profile;
    uint32_t calls, released;
} RegistryOwner;
static void registry_release(void *user) {
    RegistryOwner *owner=(RegistryOwner *)user;
    assert(owner->released==0);owner->released++;
}
static uint32_t registry_invoke(void *user,const lang_component_v1_context *context,
                                lang_component_v1_bytes input,lang_component_v1_buffer **output) {
    RegistryOwner *owner=(RegistryOwner *)user;
    lang_component_v1_error error={0};lang_component_v1_mut_bytes view;
    lang_component_v1_result *untouched=(lang_component_v1_result *)(uintptr_t)42;
    @HOST_ARGUMENTS@ arguments={0};@HOST_RESULT@ result=0;
    lc_value_reader reader={0};lc_value_writer writer={0};uint8_t scratch[1024];
    error.header.size=sizeof(error);error.header.version=1;
    assert(context->engine==owner->engine&&owner->api->context_check(context,&error)==0);
    if(owner->depth){owner->inner=*context;assert(owner->api->context_check(&owner->saved,&error)==5);}
    else owner->saved=*context;
    owner->calls++;
    assert(owner->api->provider_drop(owner->provider,&untouched,&error)==4);
    assert(untouched==(lang_component_v1_result *)(uintptr_t)42);
    reader.data=input.data;reader.length=input.length;reader.contextual=1;
    reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
    assert(@HOST_NAME@_decode_arguments(&reader,&arguments));
    assert(arguments.p0==41);result=arguments.p0+1;
    if(owner->profile==7||owner->profile==8) {
        assert(owner->api->context_buffer_create(context,4,1,output,&error)==0);
        assert(owner->api->buffer_mut_bytes(context,*output,&view,&error)==0&&view.length==4);
        memcpy(view.data,"LCMP",4);return owner->profile==7?1u:2u;
    }
    if(owner->depth&&owner->profile>=5)return probe_native_failure_report(context,output,0);
    if(!owner->depth) {
        uint32_t i;
        assert(probe_context_thread(context,owner->callback)==4);
        if(owner->retained){assert(owner->api->result_drop(owner->retained,&error)==0);owner->retained=NULL;}
        if(owner->profile==3) {
            @ROOT_ARGUMENTS@ args={0};lc_value_writer child_writer={0};lc_value_outcome child;
            lang_component_v1_bytes bytes;lang_component_v1_usage usage={0};uint8_t child_scratch[1024];
            args.p0=41;child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
            child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
            owner->api->interrupt(owner->control);
            child=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&child_writer,&error);
            assert(child.status==1&&child.commit==0&&!child.poisoned&&child.result);
            assert(owner->api->context_check(context,&error)==1);
            usage.header.size=sizeof(usage);usage.header.version=1;
            assert(owner->api->result_usage(child.result,&usage,&error)==0&&usage.host_calls==1);
            assert(owner->api->result_bytes(child.result,&bytes,&error)==0&&bytes.length>4&&memcmp(bytes.data,"LCMP",4)==0);
            owner->retained=child.result;return 1;
        }
        if(owner->profile==4)return probe_native_failure_report(context,output,0);
        if(owner->profile==9) {
            if(owner->calls==1)return probe_native_failure_report(context,output,1);
            goto encode_result;
        }
        owner->depth++;
        for(i=0;i<(owner->profile?2u:1u);++i) {
            @ROOT_ARGUMENTS@ args={0};@ROOT_RESULT@ value=0;
            lc_value_writer child_writer={0};lc_value_reader child_reader={0};
            lc_value_outcome child;lang_component_v1_bytes bytes;
            uint8_t child_scratch[1024];args.p0=41;
            child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
            child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
            child=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&child_writer,&error);
            if(owner->profile>=5) {
                lang_component_v1_mut_bytes returned;
                assert(child.status==1&&child.commit==1&&child.poisoned&&child.result);
                assert(owner->api->context_check(context,&error)==0);
                assert(owner->api->result_bytes(child.result,&bytes,&error)==0);
                assert(probe_report(bytes,0,8)==0);
                assert(owner->api->context_buffer_create(context,bytes.length,1,output,&error)==0);
                assert(owner->api->buffer_mut_bytes(context,*output,&returned,&error)==0&&returned.length==bytes.length);
                memcpy(returned.data,bytes.data,(size_t)bytes.length);
                assert(owner->api->result_drop(child.result,&error)==0);
                owner->depth--;return 1;
            }
            if(!owner->profile){assert(child.status==4&&!child.result);break;}
            assert(child.status==0&&child.commit==1&&child.result);
            assert(owner->api->context_check(context,&error)==0);
            assert(context->invocation==owner->inner.invocation&&context->scope!=owner->inner.scope);
            assert(owner->api->context_check(&owner->inner,&error)==5);
            assert(owner->api->result_bytes(child.result,&bytes,&error)==0);
            child_reader.data=bytes.data;child_reader.length=bytes.length;child_reader.contextual=1;
            child_reader.left.work=1000000;child_reader.left.elements=1000;child_reader.left.depth=128;
            assert(@ROOT_NAME@_decode_result(&child_reader,&value)&&value==42);
            if(i==0)owner->retained=child.result;
            else assert(owner->api->result_drop(child.result,&error)==0);
            result=value;
        }
        owner->depth--;
    }
encode_result:
    writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
    writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
    assert(@HOST_NAME@_encode_result(&writer,&result));
    assert(owner->api->context_buffer_create(context,writer.at,1,output,&error)==0);
    assert(owner->api->buffer_mut_bytes(context,*output,&view,&error)==0&&view.length==writer.at);
    memcpy(view.data,scratch,(size_t)writer.at);return 0;
}
static uint8_t *registry_copy(const uint8_t *bytes,uint64_t length) {
    uint8_t *copy=length?(uint8_t *)malloc((size_t)length):NULL;
    if(length){assert(copy);memcpy(copy,bytes,(size_t)length);}return copy;
}
static void registry_profile(const lang_component_v1_api *api,const lang_component_v1_config *original,uint32_t format,uint32_t profile) {
    RegistryOwner owner={0},other={0};
    lang_component_v1_config config=*original;
    lang_component_v1_provider_config registration={0};
    lang_component_v1_import *methods=(lang_component_v1_import *)calloc(2,sizeof(*methods));
    lang_component_v1_engine *engine=NULL;lang_component_v1_module *module=NULL;
    lang_component_v1_instance *instance=NULL;lang_component_v1_binding *binding=NULL;
    lang_component_v1_instance *second_instance=NULL;lang_component_v1_binding *second_binding=NULL;
    lang_component_v1_error error={0};lang_component_v1_instance_config options={0};
    lang_component_v1_result *closed=(lang_component_v1_result *)(uintptr_t)42;
    lang_component_v1_bytes request={projection,sizeof(projection)},grant={native_grant,sizeof(native_grant)};
    lang_component_v1_bytes *grants=(lang_component_v1_bytes *)calloc(1,sizeof(*grants));
    uint8_t *metadata=registry_copy(native_contract,sizeof(native_contract));
    uint8_t *key=registry_copy(native_key,sizeof(native_key));
    uint8_t *grant_copy=registry_copy(native_grant,sizeof(native_grant));
    uint8_t *program=registry_copy(format?native_artifact:native_source,format?sizeof(native_artifact):sizeof(native_source));
    uint8_t *idl_copy=format?NULL:registry_copy(native_idl,sizeof(native_idl));
    lang_component_v1_bytes program_bytes={program,format?sizeof(native_artifact):sizeof(native_source)};
    lang_component_v1_bytes idl_bytes={idl_copy,format?0:sizeof(native_idl)};
    uint32_t i;assert(methods&&grants);
    if(profile>=4){config.ceiling.suppressed_records=4;config.ceiling.trace_frames=4;}
    config.ceiling.preparation_work=10000000;config.ceiling.metadata_bytes=16777216;
    config.ceiling.serialization_bytes=16777216;config.ceiling.value_elements=1000000;
    error.header.size=sizeof(error);error.header.version=1;
    assert(api->engine_create(&config,&engine,&error)==0);
    owner.api=api;owner.engine=engine;owner.profile=profile;
    methods[0].header.size=sizeof(*methods);methods[0].header.version=1;
    methods[0].function_key.data=key;methods[0].function_key.length=sizeof(native_key);
    memcpy(methods[0].signature,native_signature,32);
    methods[0].invoke=registry_invoke;methods[0].user=&owner;
    methods[0].flags=profile?1u:0u;
    registration.header.size=sizeof(registration);registration.header.version=1;
    registration.contract.data=metadata;registration.contract.length=sizeof(native_contract);
    registration.imports=methods;registration.import_count=1;
    registration.owner=&owner;registration.release_owner=registry_release;
    for(i=0;i<7;++i) {
        lang_component_v1_provider_config bad=registration;
        lang_component_v1_provider *untouched=(lang_component_v1_provider *)(uintptr_t)42;
        lang_component_v1_import copy=methods[0];
        if(i==0)bad.reserved[0]=1;
        if(i==1){methods[0].signature[0]^=1;}
        if(i==2)bad.imports=NULL;
        if(i==3){methods[1]=methods[0];bad.import_count=2;}
        if(i==4)methods[0].invoke=NULL;
        if(i==5)methods[0].flags=2;
        if(i==6)bad.release_owner=NULL;
        assert(api->provider_register(engine,&bad,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_provider *)(uintptr_t)42&&owner.calls==0&&owner.released==0);
        methods[0]=copy;
    }
    assert(api->provider_register(engine,&registration,&owner.provider,&error)==0);
    memset(metadata,0,sizeof(native_contract));free(metadata);
    memset(key,0,sizeof(native_key));free(key);
    memset(methods,0,2*sizeof(*methods));free(methods);
    memset(&registration,0,sizeof(registration));
    assert(api->module_load(engine,format,program_bytes,idl_bytes,&module,&error)==0);
    memset(program,0,(size_t)program_bytes.length);free(program);
    if(idl_copy){memset(idl_copy,0,sizeof(native_idl));free(idl_copy);}
    options.header.size=sizeof(options);options.header.version=1;options.ceiling=config.ceiling;
    instance=(lang_component_v1_instance *)(uintptr_t)42;
    assert(api->instance_create(module,&options,&instance,&error)==2&&instance==(lang_component_v1_instance *)(uintptr_t)42);
    assert(owner.calls==0&&owner.released==0);
    grants[0].data=grant_copy;grants[0].length=sizeof(native_grant);options.grants=grants;options.grant_count=1;
    assert(api->instance_create(module,&options,&instance,&error)==0);
    memset(grant_copy,0,sizeof(native_grant));free(grant_copy);memset(grants,0,sizeof(*grants));free(grants);
    assert(api->bind_export(instance,request,&binding,&error)==0);
    options.grants=&grant;
    assert(api->instance_create(module,&options,&second_instance,&error)==0);
    assert(api->bind_export(second_instance,request,&second_binding,&error)==0);
    owner.callback=(profile==2||profile==6)?second_binding:binding;
    assert(api->provider_drop(owner.provider,&closed,&error)==4&&closed==(lang_component_v1_result *)(uintptr_t)42);
    {
        lang_component_v1_provider_config second={0};lang_component_v1_import method={0};
        lang_component_v1_instance *untouched=(lang_component_v1_instance *)(uintptr_t)42;
        other.api=api;other.engine=engine;
        method.header.size=sizeof(method);method.header.version=1;
        method.function_key.data=native_key;method.function_key.length=sizeof(native_key);
        memcpy(method.signature,native_signature,32);method.invoke=registry_invoke;method.user=&other;
        second.header.size=sizeof(second);second.header.version=1;
        second.contract.data=native_contract;second.contract.length=sizeof(native_contract);
        second.imports=&method;second.import_count=1;second.owner=&other;second.release_owner=registry_release;
        assert(api->provider_register(engine,&second,&other.provider,&error)==0);
        options.grants=&grant;
        assert(api->instance_create(module,&options,&untouched,&error)==2&&untouched==(lang_component_v1_instance *)(uintptr_t)42);
        assert(owner.calls==0&&other.calls==0&&other.released==0);
    }
    assert(api->module_drop(module,&error)==0);
    for(i=0;i<2;++i) {
        lang_component_v1_control *control=NULL;lc_value_outcome outcome;
        @ROOT_ARGUMENTS@ arguments={0};@ROOT_RESULT@ result=0;
        lc_value_writer writer={0};lc_value_reader reader={0};lang_component_v1_bytes bytes;
        uint8_t scratch[1024];arguments.p0=41;
        assert(api->control_create(engine,&config.ceiling,&control,&error)==0);
        owner.control=control;
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.left.work=1000000;
        writer.left.elements=1000;writer.left.depth=128;
        outcome=@ROOT_NAME@_call(api,control,binding,&arguments,&writer,&error);
        if(profile>=4&&profile<=8&&i==1) {
            assert(outcome.status==2&&outcome.commit==0&&!outcome.result&&owner.calls==((profile==4||profile>=7)?1u:2u));
            assert(api->control_drop(control,&error)==0);continue;
        }
        assert(outcome.status==((profile==9&&i==1)?0u:profile>=3?1u:0u)&&outcome.commit==1&&outcome.result&&outcome.poisoned==(profile>=4&&profile<=8));
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        if(profile>=3&&!(profile==9&&i==1))assert(bytes.length>4&&memcmp(bytes.data,"LCMP",4)==0);
        else assert(@ROOT_NAME@_decode_result(&reader,&result)&&result==42);
        if(profile>=4&&!(profile==9&&i==1)){assert(probe_report(bytes,config.mode,profile==4?7u:profile<7?8u:profile<9?9u:10u)==0);owner.retained=outcome.result;}
        else assert(api->result_drop(outcome.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    assert(owner.calls==((profile==4||(profile>=7&&profile<=8))?1u:profile>=5?2u:(profile&&profile!=3)?6u:2u)&&other.calls==0&&owner.released==0);
    if(profile==6) {
        lang_component_v1_control *control=NULL;@ROOT_ARGUMENTS@ arguments={0};
        lc_value_writer writer={0};lc_value_outcome outcome;uint8_t scratch[1024];
        arguments.p0=41;writer.data=scratch;writer.capacity=sizeof(scratch);
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        assert(api->control_create(engine,&config.ceiling,&control,&error)==0);
        outcome=@ROOT_NAME@_call(api,control,second_binding,&arguments,&writer,&error);
        assert(outcome.status==2&&outcome.commit==0&&!outcome.result&&owner.calls==2);
        assert(api->control_drop(control,&error)==0);
    }
    assert(api->provider_drop(other.provider,&closed,&error)==0&&closed==NULL&&other.released==1);
    assert(api->context_check(&owner.saved,&error)==5);
    {
        lang_component_v1_result *untouched=(lang_component_v1_result *)(uintptr_t)42;
        lang_component_v1_bytes empty={NULL,0};lang_component_v1_context malformed=owner.saved;
        assert(api->context_call(&owner.saved,binding,empty,&untouched,&error)==5);
        assert(untouched==(lang_component_v1_result *)(uintptr_t)42);
        malformed.reserved[0]=1;
        assert(api->context_call(&malformed,binding,empty,&untouched,&error)==2);
        assert(untouched==(lang_component_v1_result *)(uintptr_t)42);
    }
    lang_component_v1_bytes retained_view={NULL,0};
    if(owner.retained)assert(api->result_bytes(owner.retained,&retained_view,&error)==0);
    assert(api->instance_close(instance,&closed,&error)==0&&closed==NULL);
    assert(api->instance_close(second_instance,&closed,&error)==0&&closed==NULL);
    assert(api->provider_drop(owner.provider,&closed,&error)==0&&closed==NULL&&owner.released==0);
    assert(api->binding_drop(binding,&error)==0);
    assert(api->binding_drop(second_binding,&error)==0);
    if(owner.retained){
        lang_component_v1_bytes bytes;
        assert(owner.released==0);assert(api->engine_destroy(engine,&error)==4);
        assert(api->result_bytes(owner.retained,&bytes,&error)==0);
        assert(bytes.data==retained_view.data&&bytes.length==retained_view.length);
        if(profile>=4)assert(probe_report(bytes,config.mode,profile==4?7u:profile<7?8u:9u)==0);
        assert(api->result_drop(owner.retained,&error)==0);
    }
    assert(owner.released==1);
    assert(api->engine_destroy(engine,&error)==0);
}
static void registry_case(const lang_component_v1_api *api,const lang_component_v1_config *config,uint32_t format) {
    uint32_t profile;for(profile=0;profile<10;++profile)registry_profile(api,config,format,profile);
}
