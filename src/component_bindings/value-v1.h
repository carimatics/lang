#ifndef LANG_COMPONENT_VALUE_V1_H
#define LANG_COMPONENT_VALUE_V1_H
/* Generated typed views borrow frame/scratch storage. Native pointers must
 * denote the stated extents and must not alias codec state/output/scratch.
 * Contextual indices are untrusted claims; only the runtime adapter grants
 * ownership/loans. No pointer, padding or resource backing is serialized. */
#include <stddef.h>
#if defined(__clang__) || defined(__GNUC__)
#define LC_VALUE_INLINE static inline __attribute__((unused))
#else
#define LC_VALUE_INLINE static inline
#endif
#ifdef __cplusplus
#define LC_VALUE_ALIGNOF(T) alignof(T)
#else
#define LC_VALUE_ALIGNOF(T) _Alignof(T)
#endif
typedef struct lc_value_unit { uint8_t unused; } lc_value_unit;
typedef struct lc_value_limits { uint64_t work, elements, depth; } lc_value_limits;
typedef struct lc_value_writer {
    uint8_t *data; uint64_t capacity, at;
    lc_value_limits left; uint32_t status, contextual;
} lc_value_writer;
typedef struct lc_value_reader {
    const uint8_t *data; uint64_t length, at;
    uint8_t *scratch; uint64_t capacity, used;
    lc_value_limits left; uint32_t status, contextual;
} lc_value_reader;
/* Codec status: 0 success, 1 extent/truncation, 2 malformed, 3 finite limit.
 * Refusal may consume work/scratch and write scratch; no root is published. */
LC_VALUE_INLINE int lc_value_bad(uint32_t *status,uint32_t why) {
    if (*status==0) *status=why;
    return 0;
}
LC_VALUE_INLINE int lc_value_work(lc_value_limits *left,uint32_t *status,uint64_t n) {
    if (*status) return 0;
    if (n>left->work) return lc_value_bad(status,3);
    left->work-=n; return 1;
}
LC_VALUE_INLINE int lc_value_enter(lc_value_limits *left,uint32_t *status,uint64_t depth) {
    if (!lc_value_work(left,status,1)) return 0;
    if (depth==UINT64_MAX || depth>left->depth || left->elements==0) return lc_value_bad(status,3);
    --left->elements; return 1;
}
LC_VALUE_INLINE int lc_value_put(lc_value_writer *w,const uint8_t *data,uint64_t n) {
    uint64_t i;
    if (n==UINT64_MAX) return lc_value_bad(&w->status,3);
    if (!lc_value_work(&w->left,&w->status,n+1)) return 0;
    if (w->at>w->capacity || n>w->capacity-w->at || (n && (!data || !w->data))) return lc_value_bad(&w->status,1);
    for(i=0;i<n;++i) w->data[w->at+i]=data[i];
    w->at+=n; return 1;
}
LC_VALUE_INLINE int lc_value_get(lc_value_reader *r,uint64_t n,const uint8_t **data) {
    if (n==UINT64_MAX) return lc_value_bad(&r->status,3);
    if (!lc_value_work(&r->left,&r->status,n+1)) return 0;
    if (r->at>r->length || n>r->length-r->at || (n && !r->data)) return lc_value_bad(&r->status,1);
    *data=r->data ? r->data+r->at : r->data; r->at+=n; return 1;
}
LC_VALUE_INLINE int lc_value_unsigned(lc_value_writer *w,uint64_t value,uint32_t n) {
    uint8_t bytes[8]; uint32_t i;
    for(i=0;i<n;++i) {bytes[i]=(uint8_t)value;value>>=8;}
    return lc_value_put(w,bytes,n);
}
LC_VALUE_INLINE int lc_value_number(lc_value_reader *r,uint64_t *value,uint32_t n) {
    const uint8_t *bytes; uint32_t i; uint64_t result=0;
    if (!lc_value_get(r,n,&bytes)) return 0;
    for(i=0;i<n;++i) result|=((uint64_t)bytes[i])<<(8*i);
    *value=result; return 1;
}
LC_VALUE_INLINE int lc_value_utf8(const uint8_t *p,uint64_t n,lc_value_limits *left,uint32_t *status) {
    uint64_t i=0; uint32_t c,k,need,min;
    if (n==UINT64_MAX) return lc_value_bad(status,3);
    if (!lc_value_work(left,status,n+1)) return 0;
    if(n && !p) return lc_value_bad(status,1);
    while(i<n) {
        c=p[i++]; if(c<128) continue;
        if(c>=194 && c<=223) {need=1;min=128;c&=31;}
        else if(c>=224 && c<=239) {need=2;min=2048;c&=15;}
        else if(c>=240 && c<=244) {need=3;min=65536;c&=7;}
        else return lc_value_bad(status,2);
        if(need>n-i) return lc_value_bad(status,2);
        for(k=0;k<need;++k) {uint32_t b=p[i++];if(b<128 || b>191)return lc_value_bad(status,2);c=(c<<6)|(b&63);}
        if(c<min || c>1114111 || (c>=55296 && c<=57343)) return lc_value_bad(status,2);
    }
    return 1;
}
LC_VALUE_INLINE int lc_value_blob_write(lc_value_writer *w,const lang_component_v1_bytes *v,int text) {
    if(v->length>UINT32_MAX) return lc_value_bad(&w->status,2);
    if(text && !lc_value_utf8(v->data,v->length,&w->left,&w->status)) return 0;
    return lc_value_unsigned(w,v->length,4) && lc_value_put(w,v->data,v->length);
}
LC_VALUE_INLINE int lc_value_blob_read(lc_value_reader *r,lang_component_v1_bytes *v,int text) {
    uint64_t n; const uint8_t *p;
    if(!lc_value_number(r,&n,4) || !lc_value_get(r,n,&p)) return 0;
    if(text && !lc_value_utf8(p,n,&r->left,&r->status)) return 0;
    v->data=p;v->length=n;return 1;
}
LC_VALUE_INLINE void *lc_value_array(lc_value_reader *r,uint64_t count,uint64_t size,uint64_t alignment) {
    uintptr_t base,address; uint64_t padding,n;
    if(!lc_value_work(&r->left,&r->status,1))return NULL;
    if(count==0)return NULL;
    if(size==0 || count>UINT64_MAX/size || r->used>r->capacity || !r->scratch) {lc_value_bad(&r->status,3);return NULL;}
    n=count*size;base=(uintptr_t)r->scratch;
    if(r->used>UINTPTR_MAX-base) {lc_value_bad(&r->status,3);return NULL;}
    address=base+(uintptr_t)r->used;padding=(alignment-address%alignment)%alignment;
    if(padding>r->capacity-r->used || n>r->capacity-r->used-padding || padding>UINTPTR_MAX-address || n>UINTPTR_MAX-address-padding) {lc_value_bad(&r->status,3);return NULL;}
    r->used+=padding+n;return (void *)(address+(uintptr_t)padding);
}
LC_VALUE_INLINE int lc_value_frame_write(lc_value_writer *w,const uint8_t digest[32],uint32_t kind) {
    static const uint8_t magic[4]={'L','C','V','L'};
    if(w->at!=0)return lc_value_bad(&w->status,2);
    return lc_value_put(w,magic,4) && lc_value_unsigned(w,1,4) && lc_value_unsigned(w,kind,1) && lc_value_put(w,digest,32) && lc_value_unsigned(w,0,4);
}
LC_VALUE_INLINE int lc_value_frame_end(lc_value_writer *w) {
    uint64_t n;uint32_t i;
    if(w->status)return 0;
    if(w->at<45 || w->at-45>UINT32_MAX)return lc_value_bad(&w->status,2);
    if(!lc_value_work(&w->left,&w->status,5))return 0;
    n=w->at-45;for(i=0;i<4;++i){w->data[41+i]=(uint8_t)n;n>>=8;}return 1;
}
LC_VALUE_INLINE int lc_value_frame_read(lc_value_reader *r,const uint8_t digest[32],uint32_t kind) {
    const uint8_t *p;uint32_t i;uint64_t n;
    if(r->at!=0)return lc_value_bad(&r->status,2);
    if(!lc_value_get(r,45,&p))return 0;
    if(p[0]!='L'||p[1]!='C'||p[2]!='V'||p[3]!='L'||p[4]!=1||p[5]||p[6]||p[7]||p[8]!=kind)return lc_value_bad(&r->status,2);
    for(i=0;i<32;++i)if(p[9+i]!=digest[i])return lc_value_bad(&r->status,2);
    n=(uint64_t)p[41]|((uint64_t)p[42]<<8)|((uint64_t)p[43]<<16)|((uint64_t)p[44]<<24);
    if(n!=r->length-r->at)return lc_value_bad(&r->status,2);
    return 1;
}
/* commit: 0 known no commit, 1 committed, 2 unknown. Inspect/drop result
 * explicitly. A result/codec failure never silently recreates an owner. */
typedef struct lc_value_outcome {
    lang_component_v1_status status;
    uint32_t commit,poisoned;
    lang_component_v1_result *result;
} lc_value_outcome;
LC_VALUE_INLINE lc_value_outcome lc_value_observe(const lang_component_v1_api *api,lang_component_v1_status status,lang_component_v1_result *result,lang_component_v1_error *error) {
    lc_value_outcome out={status,0,0,result};
    if(status==LANG_COMPONENT_V1_OK || status==LANG_COMPONENT_V1_CALL_FAILED) {
        lang_component_v1_result_info info={0};info.header.size=sizeof(info);info.header.version=1;out.commit=2;
        if(result && api->result_info(result,&info,error)==LANG_COMPONENT_V1_OK) {out.commit=info.flags&1;out.poisoned=(info.flags>>1)&1;}
    }
    return out;
}
#endif
