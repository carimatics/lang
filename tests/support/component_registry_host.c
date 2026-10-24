/* Native C registry fixture: no public version-1 query/table. */
typedef struct {
    const lang_component_v1_api *api;
    lang_component_v1_engine *engine;
    lang_component_v1_provider *provider;
    lang_component_v1_context saved;
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
    owner->saved=*context;owner->calls++;
    assert(owner->api->provider_drop(owner->provider,&untouched,&error)==4);
    assert(untouched==(lang_component_v1_result *)(uintptr_t)42);
    reader.data=input.data;reader.length=input.length;reader.contextual=1;
    reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
    assert(@HOST_NAME@_decode_arguments(&reader,&arguments));
    assert(arguments.p0==41);result=arguments.p0+1;
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
static void registry_case(const lang_component_v1_api *api,const lang_component_v1_config *original,uint32_t format) {
    RegistryOwner owner={0},other={0};
    lang_component_v1_config config=*original;
    lang_component_v1_provider_config registration={0};
    lang_component_v1_import *methods=(lang_component_v1_import *)calloc(2,sizeof(*methods));
    lang_component_v1_engine *engine=NULL;lang_component_v1_module *module=NULL;
    lang_component_v1_instance *instance=NULL;lang_component_v1_binding *binding=NULL;
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
    config.ceiling.preparation_work=10000000;config.ceiling.metadata_bytes=16777216;
    config.ceiling.serialization_bytes=16777216;config.ceiling.value_elements=1000000;
    error.header.size=sizeof(error);error.header.version=1;
    assert(api->engine_create(&config,&engine,&error)==0);
    owner.api=api;owner.engine=engine;
    methods[0].header.size=sizeof(*methods);methods[0].header.version=1;
    methods[0].function_key.data=key;methods[0].function_key.length=sizeof(native_key);
    memcpy(methods[0].signature,native_signature,32);
    methods[0].invoke=registry_invoke;methods[0].user=&owner;
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
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.left.work=1000000;
        writer.left.elements=1000;writer.left.depth=128;
        outcome=@ROOT_NAME@_call(api,control,binding,&arguments,&writer,&error);
        assert(outcome.status==0&&outcome.commit==1&&outcome.result);
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        assert(@ROOT_NAME@_decode_result(&reader,&result)&&result==42);
        assert(api->result_drop(outcome.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    assert(owner.calls==2&&other.calls==0&&owner.released==0);
    assert(api->provider_drop(other.provider,&closed,&error)==0&&closed==NULL&&other.released==1);
    assert(api->context_check(&owner.saved,&error)==5);
    assert(api->instance_close(instance,&closed,&error)==0&&closed==NULL);
    assert(api->provider_drop(owner.provider,&closed,&error)==0&&closed==NULL&&owner.released==0);
    assert(api->binding_drop(binding,&error)==0&&owner.released==1);
    assert(api->engine_destroy(engine,&error)==0);
}
