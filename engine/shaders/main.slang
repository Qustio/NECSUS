struct PushConstants
{
    float4x4 model;
    float4x4 view;
    float4x4 proj;
};
[[vk::push_constant]]
PushConstants pc;

struct VSOutput
{
    float4 position : SV_Position;
    float4 ndc : TEXCOORD0;
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
    output.position = mul(pc.proj, mul(pc.view, mul(pc.model, float4(input.pos.xyz, 1.0))));
    output.ndc = float4(input.normal, 1.0);
    //output.position.w = 1.0;
    return output;
}

[shader("fragment")]
float4 fragMain(VSOutput input) : SV_Target
{
    return input.ndc;
}
