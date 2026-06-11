struct PushConstants
{
    float4x4 model;
    float4x4 view;
    float4x4 proj;
    float4x4 light_proj;
};
[[vk::push_constant]]
PushConstants pc;

[[vk::binding(0, 0)]] [[vk::combinedImageSampler]]
Texture2D shadowMap;
[[vk::binding(0, 0)]] [[vk::combinedImageSampler]]
SamplerComparisonState shadowSampler;

struct VSOutput
{
    float4 position : SV_Position;
    float4 ndc : TEXCOORD0;
    float4 lightSpacePos : TEXCOORD1;
};

struct VSInput
{
    float4 pos;
    float3 normal;
    float4 color;
};

[shader("vertex")]
VSOutput vertMain(VSInput input)
{
    VSOutput output;
    float4 worldPos = mul(pc.model, float4(input.pos.xyz, 1.0));
    output.position = mul(pc.proj, mul(pc.view, worldPos));
    output.ndc = float4(input.normal/2 + 0.5, 1.0);
    output.lightSpacePos = mul(pc.light_proj, worldPos);
    return output;
}

[shader("fragment")]
float4 fragMain(VSOutput input) : SV_Target
{
    float3 projCoords = input.lightSpacePos.xyz / input.lightSpacePos.w;
    float2 shadowUV = projCoords.xy * 0.5 + 0.5;
    float currentDepth = projCoords.z;
    const float bias = 0.0015;
    float lit = shadowMap.SampleCmp(shadowSampler, shadowUV, currentDepth - bias);
    float ambient = 0.2;
    float shading = ambient + (1.0 - ambient) * lit;
    float3 color = input.ndc.rgb * 0.5 + 0.5;
    return float4(color * shading, input.ndc.a);
}

[shader("vertex")]
float4 shadow(float4 pos): SV_Position
{
    float4 o = mul(pc.proj, mul(pc.view, mul(pc.model, float4(pos.xyz, 1.0))));
    return o;
}