/* Native C registry fixture: no public version-1 query/table. */
#include <stdio.h>
typedef struct {
    const lang_component_v1_api *api;
    lang_component_v1_engine *engine;
    lang_component_v1_provider *provider;
    lang_component_v1_context saved;
    lang_component_v1_context inner;
    lang_component_v1_binding *callback;
    lang_component_v1_result *retained;
    lang_component_v1_result *snapshots[2];
    lang_component_v1_bytes snapshot_views[2];
    uint8_t *snapshot_copies[2];
    lang_component_v1_control *control;
    uint32_t depth, profile, mode, allocation_case;
    uint32_t calls, released;
} RegistryOwner;
static void registry_release(void *user) {
    RegistryOwner *owner=(RegistryOwner *)user;
    assert(owner->released==0);owner->released++;
}
static uint32_t registry_deep_profile(uint32_t profile) {return profile>=35&&profile<=37;}
static uint32_t registry_failure_depth(uint32_t profile) {return profile==37?32u:2u;}
static uint32_t registry_failure_calls(uint32_t profile) {return profile==37?33u:profile>=35?3u:2u;}
static uint32_t registry_mixed_inner_case(uint32_t profile) {return profile==41?76u:profile==42?78u:80u;}
static void registry_snapshot(RegistryOwner *owner,uint32_t index,lang_component_v1_result *result,lang_component_v1_bytes bytes) {
    assert(index<2&&!owner->snapshots[index]&&bytes.length&&bytes.length<=SIZE_MAX);
    owner->snapshots[index]=result;owner->snapshot_views[index]=bytes;
    owner->snapshot_copies[index]=(uint8_t *)malloc((size_t)bytes.length);assert(owner->snapshot_copies[index]);
    memcpy(owner->snapshot_copies[index],bytes.data,(size_t)bytes.length);
}
static void registry_check_snapshots(RegistryOwner *owner) {
    uint32_t i;lang_component_v1_error error={0};error.header.size=sizeof(error);error.header.version=1;
    for(i=0;i<2;++i)if(owner->snapshots[i]){
        lang_component_v1_bytes bytes;
        assert(owner->api->result_bytes(owner->snapshots[i],&bytes,&error)==0);
        assert(bytes.data==owner->snapshot_views[i].data&&bytes.length==owner->snapshot_views[i].length);
        assert(memcmp(bytes.data,owner->snapshot_copies[i],(size_t)bytes.length)==0);
        assert(probe_report(bytes,owner->mode,owner->profile>=41?registry_mixed_inner_case(owner->profile):i?68u:67u)==0);
    }
}
static uint32_t registry_report_case(uint32_t profile) {
    if(profile==4)return 7;
    if(profile<7)return 8;
    if(profile<9)return 9;
    if(profile==9)return 10;
    if(profile==14)return 20;
    if(profile==15)return 22;
    if(profile==16)return 23;
    if(profile==17)return 24;
    if(profile==18)return 25;
    if(profile==19)return 38;
    if(profile==20)return 39;
    if(profile==21||profile==22)return 40;
    if(profile==23||profile==24)return 41;
    if(profile==25)return 55;
    if(profile==26)return 56;
    if(profile==27)return 57;
    if(profile==28)return 58;
    if(profile==29)return 59;
    if(profile==30)return 60;
    if(profile==31)return 61;
    if(profile==32)return 62;
    if(profile==33)return 63;
    if(profile==34)return 64;
    if(profile==35)return 65;
    if(profile==36)return 66;
    if(profile==37)return 166;
    if(profile==38)return 70;
    if(profile==39)return 71;
    if(profile==40)return 72;
    if(profile==41)return 75;
    if(profile==42)return 77;
    if(profile==43)return 79;
    return 11u+((profile-10u)&1u);
}
static uint32_t registry_poisoned_profile(uint32_t profile) {
    return (profile>=4&&profile<=9)||profile==15||profile==17||profile==38||profile==40;
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
    if(owner->depth&&((owner->profile>=25&&owner->profile<=26)||(registry_deep_profile(owner->profile)&&owner->depth==registry_failure_depth(owner->profile)))) {
        @ROOT_ARGUMENTS@ args={0};lc_value_writer writer={0};lc_value_outcome failed;
        lang_component_v1_bytes bytes,input;uint8_t scratch[1024];args.p0=41;
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        assert(@ROOT_NAME@_encode_arguments(&writer,&args));input.data=scratch;input.length=writer.at;
        assert(probe_exhaust_preparation(context,0)==0);
        assert(owner->api->context_call(context,owner->callback,input,&untouched,&error)!=0);
        assert(untouched==(lang_component_v1_result *)(uintptr_t)42&&owner->calls==registry_failure_calls(owner->profile));
        writer.at=0;writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        failed=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&writer,&error);
        assert(failed.status==1&&failed.commit==0&&!failed.poisoned&&failed.result);
        assert(owner->api->result_bytes(failed.result,&bytes,&error)==0&&probe_report(bytes,owner->mode,owner->profile==37?102u:owner->profile>=35?67u:54u)==0);
        if(owner->profile==35||owner->profile==36){registry_snapshot(owner,0,failed.result,bytes);return 0;}
        assert(owner->api->result_drop(failed.result,&error)==0);return 0;
    }
    if(owner->depth&&owner->profile>=41) {
        if(owner->profile==42)return probe_native_failure_report(context,output,2);
        goto encode_result;
    }
    if(owner->depth&&owner->profile>=5&&!registry_deep_profile(owner->profile))return probe_native_failure_report(context,output,0);
    if(!owner->depth||(registry_deep_profile(owner->profile)&&owner->depth<registry_failure_depth(owner->profile))) {
        uint32_t i;
        assert(probe_context_thread(context,owner->callback)==4);
        if(owner->retained&&owner->profile<25){assert(owner->api->result_drop(owner->retained,&error)==0);owner->retained=NULL;}
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
        if(owner->profile==38)return 0;
        if(owner->profile==39&&owner->calls==1){owner->api->interrupt(owner->control);return 0;}
        if(owner->profile==40){
            assert(owner->api->context_buffer_create(context,4,1,output,&error)==0);
            assert(owner->api->buffer_mut_bytes(context,*output,&view,&error)==0&&view.length==4);
            memcpy(view.data,"LCMP",4);return 0;
        }
        if(owner->profile==14||(owner->profile>=30&&owner->profile<=34))goto encode_result;
        if(owner->profile==15)return probe_native_failure_report(context,output,3);
        if(owner->profile==16){if(owner->calls==1)return probe_native_failure_report(context,output,2);goto encode_result;}
        if(owner->profile==9) {
            if(owner->calls==1)return probe_native_failure_report(context,output,1);
            goto encode_result;
        }
        if(owner->profile==18&&owner->calls==1) {
            lang_component_v1_buffer *unchanged=(lang_component_v1_buffer *)(uintptr_t)42;
            @ROOT_ARGUMENTS@ args={0};lc_value_writer report_writer={0};lc_value_outcome failed;
            lang_component_v1_bytes bytes;uint8_t report_scratch[1024];args.p0=41;
            if(owner->allocation_case==1) {
                lang_component_v1_buffer *held=NULL;lang_component_v1_mut_bytes view={0};
                assert(owner->api->context_buffer_create(context,2048,1,&held,&error)==0);
                assert(owner->api->buffer_mut_bytes(context,held,&view,&error)==0&&view.length==2048);
                memset(view.data,0xa5,(size_t)view.length);
                assert(owner->api->context_buffer_create(context,2048,1,&unchanged,&error)!=0);
                assert(view.data[0]==0xa5&&view.data[2047]==0xa5);
            } else if(owner->allocation_case==2) {
                uint32_t count;int refused=0;
                for(count=0;count<128;++count) {
                    unchanged=(lang_component_v1_buffer *)(uintptr_t)42;
                    if(owner->api->context_buffer_create(context,0,1,&unchanged,&error)!=0) {refused=1;break;}
                    assert(unchanged&&unchanged!=(lang_component_v1_buffer *)(uintptr_t)42);
                }
                assert(refused&&count>0&&count<128);
            } else assert(owner->api->context_buffer_create(context,4097,1,&unchanged,&error)!=0);
            assert(unchanged==(lang_component_v1_buffer *)(uintptr_t)42);
            assert(owner->api->context_check(context,&error)==1);
            report_writer.data=report_scratch;report_writer.capacity=sizeof(report_scratch);
            report_writer.left.work=1000000;report_writer.left.elements=1000;report_writer.left.depth=128;
            failed=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&report_writer,&error);
            assert(failed.status==1&&failed.commit==0&&!failed.poisoned&&failed.result);
            assert(owner->api->result_bytes(failed.result,&bytes,&error)==0&&probe_report(bytes,owner->mode,owner->allocation_case?199u+owner->allocation_case:25u)==0);
            assert(owner->api->result_drop(failed.result,&error)==0);
            return 0;
        }
        if(owner->profile>=41&&owner->calls==1) {
            @ROOT_ARGUMENTS@ args={0};lc_value_writer child_writer={0};lc_value_outcome child;
            lang_component_v1_bytes bytes;uint8_t child_scratch[1024];args.p0=-7;
            child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
            child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
            owner->depth++;
            child=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&child_writer,&error);
            owner->depth--;
            assert(child.status==1&&child.commit==1&&!child.poisoned&&child.result&&owner->calls==2);
            assert(owner->api->result_bytes(child.result,&bytes,&error)==0&&probe_report(bytes,owner->mode,registry_mixed_inner_case(owner->profile))==0);
            registry_snapshot(owner,0,child.result,bytes);return 0;
        }
        if(((owner->profile>=19&&owner->profile<=24)||(owner->profile>=27&&owner->profile<=29))&&owner->calls==1) {
            @ROOT_ARGUMENTS@ args={0};lc_value_writer child_writer={0};uint8_t child_scratch[1024];
            lang_component_v1_bytes input;lang_component_v1_result *unchanged=(lang_component_v1_result *)(uintptr_t)42;
            args.p0=41;child_writer.data=child_scratch;child_writer.capacity=sizeof(child_scratch);
            child_writer.left.work=1000000;child_writer.left.elements=1000;child_writer.left.depth=128;
            assert(@ROOT_NAME@_encode_arguments(&child_writer,&args));input.data=child_scratch;input.length=child_writer.at;
            assert(probe_exhaust_preparation(context,owner->profile>=27?0u:(owner->profile-19u)/2u)==0);
            assert(owner->api->context_call(context,owner->callback,input,&unchanged,&error)!=0);
            assert(unchanged==(lang_component_v1_result *)(uintptr_t)42&&owner->calls==1);
            assert(owner->api->context_check(context,&error)==1);
            {
                lc_value_outcome first,again;lang_component_v1_bytes bytes,repeated;
                lc_value_writer writer={0};writer.data=child_scratch;writer.capacity=sizeof(child_scratch);
                writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
                first=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&writer,&error);
                assert(first.status==1&&first.commit==0&&!first.poisoned&&first.result&&owner->calls==1);
                assert(owner->api->result_bytes(first.result,&bytes,&error)==0&&probe_report(bytes,owner->mode,registry_report_case(owner->profile))==0);
                writer.at=0;writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
                again=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&writer,&error);
                assert(again.status==1&&again.commit==0&&!again.poisoned&&again.result&&owner->calls==1);
                assert(owner->api->result_bytes(again.result,&repeated,&error)==0);
                assert(repeated.length==bytes.length&&memcmp(repeated.data,bytes.data,(size_t)bytes.length)==0);
                assert(owner->api->result_drop(again.result,&error)==0);
                assert(owner->api->result_drop(first.result,&error)==0);
            }
            return 0;
        }
        if((owner->profile>=25&&owner->profile<=26&&owner->calls==1)||(registry_deep_profile(owner->profile)&&owner->calls<=registry_failure_calls(owner->profile))) {
            @ROOT_ARGUMENTS@ args={0};lc_value_writer writer={0};lc_value_outcome child;
            lang_component_v1_bytes bytes;uint8_t scratch[1024];args.p0=41;
            writer.data=scratch;writer.capacity=sizeof(scratch);writer.contextual=1;
            writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
            owner->depth++;
            child=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&writer,&error);
            owner->depth--;
            if(!(child.status==1&&child.commit==1&&!child.poisoned&&child.result&&owner->calls==registry_failure_calls(owner->profile))){
                fprintf(stderr,"deep profile=%u depth=%u calls=%u expected=%u status=%u commit=%u poison=%u\n",owner->profile,owner->depth,owner->calls,registry_failure_calls(owner->profile),child.status,child.commit,child.poisoned);
                if(child.result){assert(owner->api->result_bytes(child.result,&bytes,&error)==0);assert(probe_report(bytes,owner->mode,99)==0);}
            }
            assert(child.status==1&&child.commit==1&&!child.poisoned&&child.result&&owner->calls==registry_failure_calls(owner->profile));
            assert(owner->api->result_bytes(child.result,&bytes,&error)==0&&probe_report(bytes,owner->mode,owner->profile==37?100u+2u*(registry_failure_depth(owner->profile)-owner->depth):owner->profile>=35?(owner->depth==0?68u:67u):54u)==0);
            registry_check_snapshots(owner);
            if((owner->profile==35||owner->profile==36)&&owner->depth==0){registry_snapshot(owner,1,child.result,bytes);return 0;}
            assert(owner->api->result_drop(child.result,&error)==0);return 0;
        }
        if(owner->profile>=10&&owner->calls>1)goto encode_result;
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
                assert(child.status==1&&child.commit==(owner->profile<10)&&child.poisoned==(owner->profile<10)&&child.result);
                assert(owner->api->context_check(context,&error)==1);
                assert(owner->api->result_bytes(child.result,&bytes,&error)==0);
                assert(probe_report(bytes,owner->mode,owner->profile==17?73u:owner->profile>=10?73u+((owner->profile-10u)&1u):69u)==0);
                if(owner->profile>=10) {
                    lc_value_outcome again;lang_component_v1_bytes repeated;
                    lc_value_writer again_writer={0};again_writer.data=child_scratch;again_writer.capacity=sizeof(child_scratch);
                    again_writer.left.work=1000000;again_writer.left.elements=1000;again_writer.left.depth=128;
                    again=@ROOT_NAME@_context_call(owner->api,context,owner->callback,&args,&again_writer,&error);
                    assert(again.status==1&&again.commit==0&&!again.poisoned&&again.result);
                    assert(owner->api->result_bytes(again.result,&repeated,&error)==0);
                    assert(repeated.length==bytes.length&&memcmp(repeated.data,bytes.data,(size_t)bytes.length)==0);
                    assert(owner->api->result_drop(again.result,&error)==0);
                }
                assert(owner->api->result_drop(child.result,&error)==0);
                owner->depth--;return owner->profile==17?2u:1u;
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
static void registry_profile(const lang_component_v1_api *api,const lang_component_v1_config *original,uint32_t format,uint32_t profile,uint32_t allocation_case) {
    RegistryOwner owner={0},other={0};
    lang_component_v1_config config=*original;
    lang_component_v1_provider_config registration={0};
    lang_component_v1_import *methods=(lang_component_v1_import *)calloc(2,sizeof(*methods));
    lang_component_v1_engine *engine=NULL;lang_component_v1_module *module=NULL;
    lang_component_v1_instance *instance=NULL;lang_component_v1_binding *binding=NULL;
    lang_component_v1_instance *second_instance=NULL;lang_component_v1_binding *second_binding=NULL;
    lang_component_v1_instance *task_instance=NULL;
    lang_component_v1_error error={0};lang_component_v1_instance_config options={0};
    lang_component_v1_result *closed=(lang_component_v1_result *)(uintptr_t)42;
    lang_component_v1_bytes request={projection,sizeof(projection)},grant={native_grant,sizeof(native_grant)};
    lang_component_v1_bytes *grants=(lang_component_v1_bytes *)calloc(1,sizeof(*grants));
    uint8_t *metadata=registry_copy(native_contract,sizeof(native_contract));
    uint8_t *key=registry_copy(native_key,sizeof(native_key));
    uint8_t *grant_copy=registry_copy(native_grant,sizeof(native_grant));
    const uint8_t *selected_program=profile>=30&&profile<=34?(format?native_fail_artifact:native_fail_source):profile==29?(format?native_long_artifact:native_long_source):profile>=27&&profile<=28?(format?native_stack_artifact:native_stack_source):(format?native_artifact:native_source);
    size_t selected_length=profile>=30&&profile<=34?(format?sizeof(native_fail_artifact):sizeof(native_fail_source)):profile==29?(format?sizeof(native_long_artifact):sizeof(native_long_source)):profile>=27&&profile<=28?(format?sizeof(native_stack_artifact):sizeof(native_stack_source)):(format?sizeof(native_artifact):sizeof(native_source));
    uint8_t *program=registry_copy(selected_program,selected_length);
    uint8_t *idl_copy=format?NULL:registry_copy(native_idl,sizeof(native_idl));
    lang_component_v1_bytes program_bytes={program,selected_length};
    lang_component_v1_bytes idl_bytes={idl_copy,format?0:sizeof(native_idl)};
    uint32_t i,report_case=allocation_case?199u+allocation_case:registry_report_case(profile);
    assert(methods&&grants&&allocation_case<=2&&(!allocation_case||profile==18));
    if(profile>=4){config.ceiling.suppressed_records=4;config.ceiling.trace_frames=4;}
    if(profile==26)config.ceiling.trace_frames=3;
    if(profile==27)config.ceiling.trace_frames=8;
    if(profile==28)config.ceiling.trace_frames=3;
    if(profile==29){config.ceiling.trace_frames=64;config.ceiling.frames=128;config.ceiling.live_allocations=4096;config.ceiling.heap_bytes=4194304;}
    if(profile==30)config.ceiling.frames=4;
    if(profile==31){config.ceiling.frames=4;config.ceiling.trace_frames=2;}
    if(profile==32){config.ceiling.frames=72;config.ceiling.trace_frames=64;config.ceiling.live_allocations=4096;config.ceiling.heap_bytes=4194304;}
    if(profile==34){config.ceiling.frames=4;config.ceiling.trace_frames=0;}
    if(profile==35)config.ceiling.trace_frames=8;
    if(profile==36)config.ceiling.trace_frames=5;
    if(profile==37){config.ceiling.trace_frames=64;config.ceiling.frames=64;config.ceiling.live_cells=512;config.ceiling.live_allocations=512;config.ceiling.heap_bytes=524288;}
    if(profile>=41)config.ceiling.trace_frames=8;
    if(profile==43)config.ceiling.trace_frames=3;
    if(profile>=10&&profile<14) {
        if((profile&1u)==0)config.ceiling.frames=1;
        else config.ceiling.live_cells=5;
    }
    if(profile==17)config.ceiling.frames=1;
    if(profile==18)config.ceiling.heap_bytes=4096;
    if(allocation_case==2)config.ceiling.live_allocations=128;
    config.ceiling.preparation_work=10000000;config.ceiling.metadata_bytes=16777216;
    config.ceiling.serialization_bytes=16777216;config.ceiling.value_elements=1000000;
    error.header.size=sizeof(error);error.header.version=1;
    assert(api->engine_create(&config,&engine,&error)==0);
    owner.api=api;owner.engine=engine;owner.profile=profile;owner.mode=config.mode;owner.allocation_case=allocation_case;
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
    if(profile>=41) {
        lang_component_v1_module *second_module=NULL;
        lang_component_v1_bytes input={format?native_fail_artifact:native_fail_source,format?sizeof(native_fail_artifact):sizeof(native_fail_source)};
        lang_component_v1_bytes metadata={format?NULL:native_idl,format?0:sizeof(native_idl)};
        uint8_t *input_copy=registry_copy(input.data,input.length),*metadata_copy=registry_copy(metadata.data,metadata.length);
        input.data=input_copy;metadata.data=metadata_copy;
        assert(api->module_load(engine,format,input,metadata,&second_module,&error)==0);
        memset(input_copy,0,(size_t)input.length);free(input_copy);
        if(metadata_copy){memset(metadata_copy,0,(size_t)metadata.length);free(metadata_copy);}
        assert(api->instance_create(second_module,&options,&second_instance,&error)==0);
        assert(api->module_drop(second_module,&error)==0);
    } else assert(api->instance_create(module,&options,&second_instance,&error)==0);
    assert(api->bind_export(second_instance,request,&second_binding,&error)==0);
    owner.callback=(profile==2||profile==6||(profile>=12&&profile<14)||profile==20||profile==22||profile==24||profile>=41)?second_binding:binding;
    if(profile==2) {
        lang_component_v1_module *task_module=NULL;
        lang_component_v1_binding *refused=(lang_component_v1_binding *)(uintptr_t)42;
        lang_component_v1_instance_config task_options={0};
        lang_component_v1_bytes task_input={format?task_artifact:task_source,format?sizeof(task_artifact):sizeof(task_source)};
        lang_component_v1_bytes task_metadata={format?NULL:task_idl,format?0:sizeof(task_idl)};
        lang_component_v1_bytes task_request={task_projection,sizeof(task_projection)};
        task_options.header.size=sizeof(task_options);task_options.header.version=1;task_options.ceiling=config.ceiling;
        assert(api->module_load(engine,format,task_input,task_metadata,&task_module,&error)==0);
        assert(api->instance_create(task_module,&task_options,&task_instance,&error)==0);
        assert(api->bind_export(task_instance,task_request,&refused,&error)==2);
        assert(refused==(lang_component_v1_binding *)(uintptr_t)42);
        assert(api->module_drop(task_module,&error)==0);
    }
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
        if(profile>=30&&profile<=34&&i==1)arguments.p0=42;
        if(profile==33&&i==0)arguments.p0=-7;
        {lang_component_v1_limits limits=config.ceiling;
         if(profile==14&&i==0)limits.host_calls=0;
         assert(api->control_create(engine,&limits,&control,&error)==0);}
        owner.control=control;
        writer.data=scratch;writer.capacity=sizeof(scratch);writer.left.work=1000000;
        writer.left.elements=1000;writer.left.depth=128;
        outcome=@ROOT_NAME@_call(api,control,binding,&arguments,&writer,&error);
        if(registry_poisoned_profile(profile)&&i==1) {
            assert(outcome.status==2&&outcome.commit==0&&!outcome.result&&owner.calls==((profile==4||profile>=7)?1u:2u));
            assert(api->control_drop(control,&error)==0);continue;
        }
        assert(outcome.status==((profile>=9&&i==1)?0u:profile>=3?1u:0u)&&outcome.commit==1&&outcome.result&&outcome.poisoned==registry_poisoned_profile(profile));
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        if(profile>=3&&!(profile>=9&&i==1))assert(bytes.length>4&&memcmp(bytes.data,"LCMP",4)==0);
        else assert(@ROOT_NAME@_decode_result(&reader,&result)&&result==42);
        if(profile>=4&&!(profile>=9&&i==1)){assert(probe_report(bytes,config.mode,report_case)==0);owner.retained=outcome.result;}
        else assert(api->result_drop(outcome.result,&error)==0);
        assert(api->control_drop(control,&error)==0);
    }
    assert(owner.calls==((profile==25||profile==26||profile>=41)?3u:registry_deep_profile(profile)?registry_failure_calls(profile)+1u:(profile==4||profile==14||profile==15||profile==17||profile==38||profile==40||(profile>=7&&profile<=9))?1u:profile>=5?2u:(profile&&profile!=3)?6u:2u)&&other.calls==0&&owner.released==0);
    registry_check_snapshots(&owner);
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
    if((profile>=12&&profile<14)||profile>=41) {
        lang_component_v1_control *control=NULL;@ROOT_ARGUMENTS@ arguments={0};@ROOT_RESULT@ value=0;
        lc_value_writer writer={0};lc_value_reader reader={0};lc_value_outcome outcome;
        lang_component_v1_bytes bytes;uint8_t scratch[1024];arguments.p0=profile>=41?42:41;
        writer.data=scratch;writer.capacity=sizeof(scratch);
        writer.left.work=1000000;writer.left.elements=1000;writer.left.depth=128;
        assert(api->control_create(engine,&config.ceiling,&control,&error)==0);
        outcome=@ROOT_NAME@_call(api,control,second_binding,&arguments,&writer,&error);
        assert(outcome.status==0&&outcome.commit==1&&!outcome.poisoned&&outcome.result&&owner.calls==(profile>=41?4u:3u));
        assert(api->result_bytes(outcome.result,&bytes,&error)==0);
        reader.data=bytes.data;reader.length=bytes.length;reader.contextual=1;
        reader.left.work=1000000;reader.left.elements=1000;reader.left.depth=128;
        assert(@ROOT_NAME@_decode_result(&reader,&value)&&value==42);
        assert(api->result_drop(outcome.result,&error)==0);
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
    if(task_instance) {
        assert(api->instance_close(task_instance,&closed,&error)==0&&closed==NULL);
    }
    assert(api->provider_drop(owner.provider,&closed,&error)==0&&closed==NULL&&owner.released==0);
    assert(api->binding_drop(binding,&error)==0);
    assert(api->binding_drop(second_binding,&error)==0);
    if(owner.retained){
        lang_component_v1_bytes bytes;
        assert(owner.released==0);assert(api->engine_destroy(engine,&error)==4);
        assert(api->result_bytes(owner.retained,&bytes,&error)==0);
        assert(bytes.data==retained_view.data&&bytes.length==retained_view.length);
        if(profile>=4)assert(probe_report(bytes,config.mode,report_case)==0);
        assert(api->result_drop(owner.retained,&error)==0);
    }
    registry_check_snapshots(&owner);
    for(i=0;i<2;++i)if(owner.snapshots[i]){
        assert(owner.released==0);
        assert(api->result_drop(owner.snapshots[i],&error)==0);free(owner.snapshot_copies[i]);
    }
    assert(owner.released==1);
    assert(api->engine_destroy(engine,&error)==0);
}
static void registry_case(const lang_component_v1_api *api,const lang_component_v1_config *config,uint32_t format) {
    uint32_t profile;for(profile=0;profile<44;++profile)registry_profile(api,config,format,profile,0);
    registry_profile(api,config,format,18,1);
    registry_profile(api,config,format,18,2);
}
